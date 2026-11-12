"use client";

import { RiArrowLeftLine, RiErrorWarningLine, RiReplyLine } from "@remixicon/react";
import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import { MessageBlocks } from "@/components/thread/message-blocks";
import { ThreadComposer } from "@/components/thread/thread-composer";
import {
  botsGet,
  normalizeMessage,
  threadsMessages,
  threadsSend,
  threadsSubscribe,
  type BotReply,
  type EventView,
  type MessageView,
} from "@/lib/api/rpc";
import { botIdFromPath, registerBot } from "@/lib/bot-directory";
import { runStatusKey } from "@/lib/run-status";
import { useTranslation } from "@/lib/use-translation";

type Phase = "loading" | "ready" | "error";
type Live = "connecting" | "live" | "offline";

/** How many times the client tries to reopen a stream that closed. */
const RECONNECT_ATTEMPTS = 3;

/**
 * The thread screen (spec/08-ui-ux.md §8.3, REQ-001, REQ-005, REQ-020).
 *
 * The screen owns three pieces of state: the bot, the transcript and the live connection. It
 * renders them as the spec describes — a header with the bot's name and its state, a transcript
 * that is a log, and a composer that stays usable whatever happens.
 *
 * The transcript is a log: the region announces what arrives, and the messages are a real list,
 * so the counts the goldens assert are the counts a person sees.
 */
export function ThreadScreen({ botId }: { botId: string }) {
  const t = useTranslation();

  const [bot, setBot] = useState<BotReply | null>(null);
  const [messages, setMessages] = useState<MessageView[]>([]);
  const [phase, setPhase] = useState<Phase>("loading");
  const [live, setLive] = useState<Live>("connecting");
  const [runStatus, setRunStatus] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [sendFailed, setSendFailed] = useState(false);
  const [reloadToken, setReloadToken] = useState(0);
  const [replyTo, setReplyTo] = useState<MessageView | null>(null);

  const lastSeq = useRef(0);
  const threadIdRef = useRef<string | null>(null);

  const applyMessage = useCallback((message: MessageView) => {
    setMessages((current) => {
      const index = current.findIndex((entry) => entry.id === message.id);
      const next =
        index === -1
          ? [...current, message]
          : current.map((entry, position) => (position === index ? message : entry));
      return [...next].sort((left, right) => left.seq - right.seq);
    });
  }, []);

  // The "try again" button resets the view itself (it is an event handler); the effect only
  // fetches. The static export carries one shell for the whole route, so the id is read from the
  // URL when there is a window.
  const reload = useCallback(() => {
    setPhase("loading");
    setSendFailed(false);
    setReloadToken((token) => token + 1);
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    const id = botIdFromPath() ?? botId;

    const load = async () => {
      const reply = await botsGet({ botId: id }, controller.signal);
      if (controller.signal.aborted) {
        return;
      }
      setBot(reply);
      registerBot({ id: reply.id, name: reply.name });
      threadIdRef.current = reply.threadId;

      const page = await threadsMessages({ threadId: reply.threadId }, controller.signal);
      if (controller.signal.aborted) {
        return;
      }
      // The server answers newest first; the transcript shows the newest at the bottom.
      const ordered = [...page.messages].reverse();
      setMessages(ordered);
      lastSeq.current = ordered.reduce((highest, message) => Math.max(highest, message.seq), 0);
      setPhase("ready");
    };

    load().catch(() => {
      if (!controller.signal.aborted) {
        setPhase("error");
      }
    });

    return () => {
      controller.abort();
    };
  }, [botId, reloadToken]);

  // The live stream. The client keeps working when the stream is down: the hint says so, and the
  // composer still sends.
  useEffect(() => {
    const threadId = bot?.threadId;
    if (!threadId || phase !== "ready") {
      return;
    }

    const controller = new AbortController();
    let stopped = false;
    let attempts = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const connect = async () => {
      if (stopped) {
        return;
      }
      try {
        await threadsSubscribe(
          { threadId, afterSeq: lastSeq.current },
          {
            onFrame: (event: EventView) => {
              lastSeq.current = Math.max(lastSeq.current, event.seq);
              if (event.type === "message" || event.type === "message-update") {
                const message = normalizeMessage(event.payload);
                if (message) {
                  applyMessage(message);
                } else {
                  // The event only names the message: the page is read again.
                  threadsMessages({ threadId }, controller.signal)
                    .then((page) => {
                      const ordered = [...page.messages].reverse();
                      setMessages(ordered);
                    })
                    .catch(() => undefined);
                }
              } else if (event.type === "run") {
                const payload = event.payload as { status?: unknown } | null;
                if (payload && typeof payload.status === "string") {
                  setRunStatus(payload.status);
                }
              }
            },
            onClose: () => {
              if (stopped) {
                return;
              }
              setLive("offline");
              attempts += 1;
              if (attempts <= RECONNECT_ATTEMPTS) {
                timer = setTimeout(() => {
                  void connect();
                }, 1000 * attempts);
              }
            },
          },
          controller.signal,
        );
      } catch {
        if (!stopped) {
          setLive("offline");
        }
      }
    };

    void connect();

    return () => {
      stopped = true;
      controller.abort();
      if (timer !== undefined) {
        clearTimeout(timer);
      }
    };
  }, [applyMessage, bot?.threadId, phase]);

  const send = useCallback(async () => {
    const text = draft.trim();
    if (text.length === 0 || sending) {
      return;
    }
    setSending(true);
    setSendFailed(false);
    try {
      const quoted = replyTo ?? null;
      const quotedText =
        quoted?.blocks.find((block) => block.type === "text")?.text.slice(0, 80) ?? "";
      const reply = await threadsSend({
        botId,
        text,
        clientNonce: `web-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`,
        ...(quoted
          ? {
              replyToMessageId: quoted.id,
              replyQuote: quotedText,
            }
          : {}),
      });
      setDraft("");
      setReplyTo(null);
      setRunStatus(reply.status);
      applyMessage({
        id: reply.messageId,
        seq: lastSeq.current + 1,
        role: "user",
        blocks: [{ type: "text", text }],
        runId: reply.runId,
        createdAt: new Date().toISOString(),
      });
    } catch {
      setSendFailed(true);
    } finally {
      setSending(false);
    }
  }, [applyMessage, botId, draft, sending, replyTo]);

  const runKey = runStatus === null ? null : runStatusKey(runStatus);
  const runLabel = runStatus === null ? null : (runKey === null ? runStatus : t(runKey));

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      <header className="flex flex-col gap-2">
        <Link
          href="/app/"
          className="boma-target boma-focus text-text-secondary hover:text-text-primary inline-flex w-fit items-center gap-1 rounded-md px-2 text-body-2-medium"
        >
          <RiArrowLeftLine className="text-foreground-icon-secondary" size={18} aria-hidden />
          {t("thread.back")}
        </Link>
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <h1 className="text-title-3-medium text-text-primary">{bot?.name ?? botId}</h1>
          {bot ? (
            <p className="text-text-secondary text-caption-1-semibold">
              {bot.archivedAt ? t("thread.bot.archived") : t("thread.bot.active")}
            </p>
          ) : null}
        </div>
      </header>

      <div
        role="log"
        aria-live="polite"
        aria-label={t("thread.transcript")}
        className="border-border-button-default bg-background-primary-default flex flex-1 flex-col gap-4 rounded-3xl border p-4"
      >
        {phase === "loading" ? <TranscriptSkeleton /> : null}

        {phase === "error" ? (
          <div className="border-border-error-default flex flex-col items-start gap-2 rounded-xl border p-3">
            <p className="text-text-error-primary text-body-medium flex items-center gap-2">
              <RiErrorWarningLine className="text-foreground-icon-error" size={18} aria-hidden />
              {t("thread.error.load")}
            </p>
            <Button variant="secondary" size="small" onClick={reload}>
              {t("common.retry")}
            </Button>
          </div>
        ) : null}

        {messages.length > 0 ? (
          <ul className="flex flex-col gap-4">
            {messages.map((message) => (
              <li key={message.id} className="flex flex-col gap-1">
                <div className="flex items-center gap-2">
                  <p className="text-text-tertiary text-caption-1-semibold">
                    {t(`thread.role.${message.role}`)}
                  </p>
                  <button
                    type="button"
                    onClick={() => setReplyTo(message)}
                    aria-label={t("thread.reply.button")}
                    className="boma-focus rounded-md p-1 text-text-tertiary hover:bg-background-secondary-hover hover:text-text-primary"
                  >
                    <RiReplyLine size={14} aria-hidden />
                  </button>
                </div>
                <div
                  className={
                    message.role === "user"
                      ? "bg-background-secondary-default flex flex-col gap-1 self-end rounded-3xl px-3 py-2 md:max-w-[80%]"
                      : "flex flex-col gap-1 self-start md:max-w-[80%]"
                  }
                >
                  {message.replyQuote ? (
                    <blockquote className="border-border-button-default border-l-2 ps-2 text-text-secondary text-body-2-regular">
                      {message.replyQuote}
                    </blockquote>
                  ) : null}
                  <MessageBlocks blocks={message.blocks} />
                </div>
              </li>
            ))}
          </ul>
        ) : null}

        {phase === "ready" && messages.length === 0 ? <EmptyThread onPick={setDraft} /> : null}

        {sendFailed ? (
          <div className="border-border-error-default flex flex-col items-start gap-2 rounded-xl border p-3">
            <p className="text-text-error-primary text-body-medium flex items-center gap-2">
              <RiErrorWarningLine className="text-foreground-icon-error" size={18} aria-hidden />
              {t("thread.error.send")}
            </p>
            <Button variant="secondary" size="small" onClick={() => void send()}>
              {t("common.retry")}
            </Button>
          </div>
        ) : null}
      </div>

      {live === "offline" ? (
        <p aria-live="polite" className="text-text-secondary text-caption-1-regular">
          {t("thread.offline")}
        </p>
      ) : null}

      {runLabel !== null ? (
        <p className="text-text-secondary text-caption-1-semibold">
          {t("thread.run.label")}: {runLabel}
        </p>
      ) : null}

      <ThreadComposer
        value={draft}
        onChange={setDraft}
        onSend={() => void send()}
        sending={sending}
        reply={
          replyTo
            ? {
                id: replyTo.id,
                quote:
                  replyTo.blocks.find((block) => block.type === "text")?.text.slice(0, 80) ?? "",
                onClear: () => setReplyTo(null),
              }
            : null
        }
      />
    </div>
  );
}

/** The skeleton of a transcript: rows shaped like messages, never a blank panel. */
function TranscriptSkeleton() {
  const t = useTranslation();

  return (
    <div className="flex flex-col gap-3">
      <p className="text-text-tertiary text-caption-1-regular">{t("thread.loading")}</p>
      <div aria-hidden="true" className="flex flex-col gap-3">
        <div className="bg-background-secondary-default h-16 w-2/3 rounded-3xl" />
        <div className="bg-background-secondary-default h-16 w-1/2 self-end rounded-3xl" />
        <div className="bg-background-secondary-default h-16 w-3/4 rounded-3xl" />
      </div>
    </div>
  );
}

/** The empty thread: the bot's intro and two prompts that fill the composer. */
function EmptyThread({ onPick }: { onPick: (value: string) => void }) {
  const t = useTranslation();

  return (
    <div className="flex flex-col items-start gap-4 py-6">
      <p className="text-text-primary text-body-medium max-w-prose">{t("thread.empty.intro")}</p>
      <div className="flex flex-col gap-2">
        <p className="text-text-secondary text-caption-1-semibold">{t("thread.empty.suggestions")}</p>
        <div className="flex flex-wrap gap-2">
          {[t("thread.empty.suggestion.1"), t("thread.empty.suggestion.2")].map((prompt) => (
            <Button
              key={prompt}
              variant="secondary"
              size="small"
              onClick={() => {
                onPick(prompt);
              }}
            >
              {prompt}
            </Button>
          ))}
        </div>
      </div>
    </div>
  );
}
