"use client";

import { useState } from "react";

import {
  RiAlertLine,
  RiCheckboxLine,
  RiFileTextLine,
  RiImageLine,
  RiTerminalLine,
} from "@remixicon/react";

import { Button } from "@/components/base/buttons/button";
import { Input } from "@/components/base/input/input";
import { RpcError, threadsAnswer, type Block } from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

/**
 * The blocks of one message (spec/08-ui-ux.md §8.3).
 *
 * Text is plain text: there is no markdown engine in M1, and the text a bot writes is data, not
 * markup. The other blocks are one line each, except the tool call, which is a disclosure: one
 * line when it is collapsed, the details when it is opened.
 */
export function MessageBlocks({
  blocks,
  onCardAnswered,
}: {
  blocks: readonly Block[];
  /** Called once per answer, after the server recorded it — lets the screen lift the gate. */
  onCardAnswered?: (cardId: string, answer: string) => void;
}) {
  return (
    <div className="flex flex-col gap-2">
      {blocks.map((block, index) => (
        <BlockView key={`${block.type}-${index}`} block={block} onCardAnswered={onCardAnswered} />
      ))}
    </div>
  );
}

function BlockView({
  block,
  onCardAnswered,
}: {
  block: Block;
  onCardAnswered?: (cardId: string, answer: string) => void;
}) {
  const t = useTranslation();

  switch (block.type) {
    case "text":
      return (
        <p className="text-text-primary text-body-regular whitespace-pre-wrap break-words">
          {block.text}
        </p>
      );

    case "image":
      return (
        <p className="text-text-secondary text-body-2-regular flex items-center gap-2">
          <RiImageLine className="text-foreground-icon-secondary" size={18} aria-hidden />
          <span>{t("thread.block.image")}</span>
          <span className="text-text-tertiary break-all">{block.artifactId}</span>
        </p>
      );

    case "file":
      return (
        <p className="text-text-secondary text-body-2-regular flex items-center gap-2">
          <RiFileTextLine className="text-foreground-icon-secondary" size={18} aria-hidden />
          <span className="text-text-primary break-all">{block.name}</span>
          <span>{t("thread.block.file")}</span>
        </p>
      );

    case "tool":
      return (
        <details className="border-separator-border rounded-xl border px-3 py-2">
          <summary className="boma-focus text-text-secondary flex cursor-pointer items-center gap-2 text-body-2-medium">
            <RiTerminalLine className="text-foreground-icon-secondary" size={18} aria-hidden />
            <span>{t("thread.block.tool")}</span>
            <span className="text-text-primary">{block.name}</span>
            <span className="text-text-tertiary">
              {block.running ? t("thread.tool.running") : t("thread.tool.finished")}
            </span>
          </summary>
          <p className="text-text-secondary text-body-2-regular mt-2">
            {t("thread.block.tool")}: {block.name}
          </p>
        </details>
      );

    case "card":
      return <CardBlock block={block} onCardAnswered={onCardAnswered} />;

    default:
      return null;
  }
}


type CardBlockProps = {
  block: Extract<Block, { type: "card" }>;
  onCardAnswered?: (cardId: string, answer: string) => void;
};

/**
 * One waiting card (spec §8.3). The blocking kinds are `approval` and `purchase`; `question`
 * takes a free-text answer; `takeover` belongs to the computer machinery and keeps its banner.
 * Answering goes through `threads.answer`; the server enforces the gates, the card only collects
 * the decision and unlocks the composer once the answer is recorded.
 */
function CardBlock({ block, onCardAnswered }: CardBlockProps) {
  const t = useTranslation();
  const [busy, setBusy] = useState(false);
  const [answerGiven, setAnswerGiven] = useState<string | null>(null);
  const [failure, setFailure] = useState<"failed" | "already" | null>(null);
  const [draft, setDraft] = useState("");

  const isApproval = block.kind === "approval" || block.kind === "purchase";
  const isQuestion = block.kind === "question";

  const explainKey =
    block.kind === "approval"
      ? "thread.card.approval.explain"
      : block.kind === "purchase"
        ? "thread.card.purchase.explain"
        : "thread.card.question.explain";

  const answer = async (value: string) => {
    if (busy || answerGiven !== null) {
      return;
    }
    setBusy(true);
    setFailure(null);
    try {
      await threadsAnswer({ cardId: block.cardId, answer: value });
      setAnswerGiven(value);
      onCardAnswered?.(block.cardId, value);
    } catch (error) {
      if (error instanceof RpcError && error.code === "CONFLICT") {
        // Already answered elsewhere (or by another device): unlock the gate honestly.
        setAnswerGiven("answered");
        setFailure("already");
        onCardAnswered?.(block.cardId, "answered");
      } else {
        setFailure("failed");
      }
    } finally {
      setBusy(false);
    }
  };

  const answeredLabel =
    answerGiven === "approve"
      ? t("thread.card.approved")
      : answerGiven === "deny"
        ? t("thread.card.denied")
        : t("thread.card.answered");

  return (
    <section
      role="region"
      aria-label={t("thread.card.region")}
      className="boma-card-in bg-background-secondary-default border-separator-border border rounded-3xl p-3 flex flex-col gap-2"
    >
      <p className="text-text-primary text-body-medium">{t("thread.block.card")}</p>
      {answerGiven === null ? (
        <>
          <p className="text-text-secondary text-body-2-regular">{t(explainKey)}</p>
          <p role="status" className="text-text-secondary text-caption-1-semibold">
            {t("thread.card.waiting")}
          </p>
          {isApproval ? (
            <>
              <p className="text-text-primary text-body-2-regular">{t("thread.card.effect.approve")}</p>
              <p className="text-text-secondary text-body-2-regular">{t("thread.card.reversal")}</p>
            </>
          ) : null}
          {failure === "failed" ? (
            <p role="alert" className="text-text-error-primary text-body-2-medium flex items-center gap-1">
              <RiAlertLine size={16} aria-hidden />
              {t("thread.card.answer.failed")}
            </p>
          ) : null}
          {isApproval ? (
            <div className="flex flex-wrap items-center gap-2">
              <Button type="button" size="small" disabled={busy} onClick={() => void answer("approve")}>
                {t("thread.card.approve")}
              </Button>
              <Button
                type="button"
                size="small"
                variant="secondary"
                disabled={busy}
                onClick={() => void answer("deny")}
              >
                {t("thread.card.deny")}
              </Button>
            </div>
          ) : (
            <form
              className="flex items-end gap-2"
              onSubmit={(event) => {
                event.preventDefault();
                // REQ-017: an empty answer is never turned into an implicit approval (ADR-033).
                const text = draft.trim();
                if (text.length > 0) {
                  void answer(text);
                }
              }}
            >
              <Input
                label={t("thread.card.answer")}
                value={draft}
                onChange={setDraft}
                placeholder={t("thread.card.answer")}
                size="small"
                className="min-w-0 flex-1"
              />
              <Button type="submit" size="small" disabled={busy || draft.trim().length === 0}>
                {t("thread.card.submit")}
              </Button>
            </form>
          )}
          {isQuestion && answerGiven === null && draft.trim().length === 0 ? (
            <p className="text-text-secondary text-caption-1-regular">{t("thread.card.answer.hint")}</p>
          ) : null}
        </>
      ) : (
        <p className="text-text-secondary text-body-medium flex items-center gap-2">
          {answerGiven === "approve" ? (
            <svg
              viewBox="0 0 24 24"
              width={18}
              height={18}
              aria-hidden
              className="animate-check-draw text-text-primary"
            >
              <polyline
                points="20 6 9 17 4 10"
                fill="none"
                stroke="currentColor"
                strokeWidth={2.4}
                strokeLinecap="round"
                strokeLinejoin="round"
                pathLength={1}
              />
            </svg>
          ) : null}
          <span>{answeredLabel}</span>
          {failure === "already" ? (
            <span className="text-text-tertiary text-body-2-regular">
              {t("thread.card.already.answered")}
            </span>
          ) : null}
        </p>
      )}
    </section>
  );
}
