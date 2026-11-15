"use client";

import Link from "next/link";
import { RiArrowLeftLine, RiSendPlane2Fill } from "@remixicon/react";
import { useEffect, useRef, useState } from "react";

import {
  groupsGet,
  groupsList,
  groupsMessages,
  threadsSend,
  type MessageView,
  type GroupMessageView,
  type GroupsListReply,
} from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

type Phase = "loading" | "ready" | "unavailable" | "error";

/**
 * `/app/g/<groupId>` (spec/08-ui-ux.md §8.3, REQ-010).
 *
 * The group thread: one shared conversation, several bots. The messages endpoint is the
 * same engine, with `botId` attached to bot messages so a reader can follow which bot said
 * what. The composer posts to the shared thread — the server routes the mention to the
 * right bots (REQ-010 mention routing).
 */
export function GroupThreadScreen({ groupId }: { groupId: string }) {
  const t = useTranslation();
  const [phase, setPhase] = useState<Phase>("loading");
  const [group, setGroup] = useState<{
    id: string;
    name: string;
    threadId: string;
    memberBots: Array<{ id: string; name: string }>;
  } | null>(null);
  const [messages, setMessages] = useState<GroupMessageView[]>([]);
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [attribution, setAttribution] = useState<Record<string, string>>({});
  const logRef = useRef<HTMLOListElement>(null);

  useEffect(() => {
    const controller = new AbortController();
    groupsGet({ id: groupId }, controller.signal)
      .then(async (body) => {
        if (controller.signal.aborted) {
          return;
        }
        setGroup(body);
        const map: Record<string, string> = {};
        for (const bot of body.memberBots) {
          map[bot.id] = bot.name;
        }
        setAttribution(map);
        const page = await groupsMessages(
          { threadId: body.threadId },
          controller.signal,
        );
        if (controller.signal.aborted) {
          return;
        }
        setMessages([...page.messages].reverse());
        setPhase("ready");
      })
      .catch(() => {
        if (!controller.signal.aborted) {
          setPhase("unavailable");
        }
      });
    return () => controller.abort();
  }, [groupId]);

  const send = async () => {
    const next = text.trim();
    if (!next || sending || !group) {
      return;
    }
    setSending(true);
    try {
      // threads/send routes through `bot.find` -> group resolution -> fan-out by mention;
      const memberBot = group.memberBots[0];
      if (!memberBot) {
        return;
      }
      await threadsSend({
        botId: memberBot.id,
        text: next,
        clientNonce: `web-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`,
      });
      setText("");
      // Reload the messages to pick up the new User row.
      const page = await groupsMessages({ threadId: group.threadId });
      setMessages([...page.messages].reverse());
    } catch {
      // Silent: the room exists, the send may be refused for rule reasons the member should
      // see after their answer. The validation errors surface elsewhere via the wire.
    } finally {
      setSending(false);
    }
  };

  if (phase === "unavailable") {
    return (
      <div className="mx-auto flex w-full max-w-2xl flex-col gap-4 p-4 sm:p-6">
        <p className="text-text-primary text-body-regular">{t("groups.unavailable")}</p>
        <Link href="/app/" className="boma-target boma-focus text-text-secondary hover:text-text-primary">
          {t("shell.rail.bots")}
        </Link>
      </div>
    );
  }

  if (phase !== "ready" || !group) {
    return (
      <div className="mx-auto flex w-full max-w-2xl flex-col gap-4 p-4 sm:p-6" aria-busy="true">
        <div className="bg-background-secondary-default h-8 w-48 rounded-md" />
        <div className="bg-background-secondary-default h-64 w-full rounded-2xl" />
      </div>
    );
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-4 p-4 sm:p-6">
      <header className="flex flex-col gap-2">
        <Link
          href="/app/"
          className="boma-target boma-focus text-text-secondary hover:text-text-primary inline-flex w-fit items-center gap-1 rounded-md px-2 text-body-2-medium"
        >
          <RiArrowLeftLine className="text-foreground-icon-secondary" size={18} aria-hidden />
          {t("thread.back")}
        </Link>
        <div className="flex flex-wrap items-center gap-3">
          <h1 className="text-title-3-medium text-text-primary break-words">{group.name}</h1>
          <div className="flex flex-wrap items-center gap-1" aria-label={t("groups.members")}>
            {group.memberBots.map((bot) => (
              <span
                key={bot.id}
                className="bg-background-secondary-default text-text-secondary inline-flex items-center rounded-full px-2 py-0.5 text-caption-1-semibold"
              >
                {bot.name}
              </span>
            ))}
          </div>
        </div>
      </header>

      <ol
        ref={logRef}
        role="log"
        aria-live="polite"
        aria-label={t("groups.transcript")}
        className="border-border-button-default bg-background-primary-default flex flex-1 flex-col gap-3 overflow-y-auto rounded-3xl border p-4"
      >
        {messages.length === 0 ? (
          <li className="text-text-tertiary text-body-regular">{t("groups.empty")}</li>
        ) : (
          messages.map((message) => (
            <li key={message.id} className="text-body-regular text-text-primary flex flex-col gap-0.5">
              <span className="text-text-tertiary text-caption-1-semibold">
                {message.role === "bot" && message.botId
                  ? (attribution[message.botId] ?? message.botId)
                  : t("thread.role.user")}
              </span>
              <span className="whitespace-pre-wrap break-words">
                {message.blocks.map((block) => (block.type === "text" ? block.text : "")).join("\n")}
              </span>
            </li>
          ))
        )}
      </ol>

      <form
        className="border-border-button-default bg-background-primary-default flex items-end gap-2 rounded-3xl border p-3"
        onSubmit={(event) => {
          event.preventDefault();
          void send();
        }}
      >
        <label htmlFor="group-composer" className="sr-only">
          {t("thread.composer.label")}
        </label>
        <input
          id="group-composer"
          type="text"
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder={t("groups.composer.placeholder")}
          disabled={sending}
          className="placeholder:text-text-placeholder flex-1 border-0 bg-transparent text-body-regular outline-none disabled:opacity-50"
        />
        <button
          type="submit"
          disabled={sending || text.trim().length === 0}
          className="boma-focus inline-flex h-9 w-9 items-center justify-center rounded-full text-foreground-icon-primary disabled:opacity-50"
          aria-label={t("thread.composer.send")}
        >
          <RiSendPlane2Fill size={18} aria-hidden />
        </button>
      </form>
    </div>
  );
}
