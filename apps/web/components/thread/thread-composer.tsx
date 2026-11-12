"use client";

import { RiCloseLine, RiReplyLine, RiSendPlaneLine } from "@remixicon/react";
import { Label, TextArea, TextField } from "react-aria-components";

import { Button } from "@/components/base/buttons/button";
import { useTranslation } from "@/lib/use-translation";

/** The message a new one answers, as the composer shows it. */
export interface ReplyContext {
  /** The message being answered. */
  id: string;
  /** The short quote the client renders. */
  quote: string;
  /** Clears the reply context. */
  onClear: () => void;
}

export interface ThreadComposerProps {
  /** What the person wrote. */
  value: string;
  /** Called on every keystroke. */
  onChange: (value: string) => void;
  /** Called when the person sends. */
  onSend: () => void;
  /** Whether a send is in flight. */
  sending: boolean;
  /** The message a new one answers, when it is a reply. */
  reply?: ReplyContext | null;
}

/**
 * The composer (spec/08-ui-ux.md §8.3).
 *
 * The text field is a real text field from React Aria, so it carries the `textbox` role, the
 * label and the keyboard behaviour a screen reader expects. The send button is disabled only
 * when there is nothing to send: a failed send never takes the keyboard away from the person.
 */
export function ThreadComposer({ value, onChange, onSend, sending, reply }: ThreadComposerProps) {
  const t = useTranslation();
  const canSend = value.trim().length > 0 && !sending;

  return (
    <form
      className="border-border-button-default bg-background-primary-default flex flex-col gap-2 rounded-3xl border p-2"
      onSubmit={(event) => {
        event.preventDefault();
        if (canSend) {
          onSend();
        }
      }}
    >
      {reply ? (
        <div className="flex items-center gap-2 rounded-2xl border border-border-button-default px-3 py-2">
          <RiReplyLine className="text-foreground-icon-secondary" size={16} aria-hidden />
          <p className="text-text-secondary text-caption-1-regular min-w-0 flex-1 truncate">
            {t("thread.reply.banner")}: {reply.quote}
          </p>
          <button
            type="button"
            onClick={reply.onClear}
            aria-label={t("thread.reply.clear")}
            className="boma-focus rounded-md p-1 text-text-secondary hover:bg-background-secondary-hover hover:text-text-primary"
          >
            <RiCloseLine size={16} aria-hidden />
          </button>
        </div>
      ) : null}
      <div className="flex items-end gap-2">
        <TextField
        value={value}
        onChange={onChange}
        className="flex min-w-0 flex-1 flex-col gap-1"
        isDisabled={false}
      >
        <Label className="sr-only">{t("thread.composer.label")}</Label>
        <TextArea
          rows={2}
          placeholder={t("thread.composer.placeholder")}
          onKeyDown={(event) => {
            // Enter sends, Shift+Enter writes a new line: the habit of every chat.
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              if (canSend) {
                onSend();
              }
            }
          }}
          className="text-text-primary text-body-regular placeholder:text-text-placeholder w-full resize-none rounded-xl bg-transparent px-2 py-1 outline-none focus-visible:ring-2 focus-visible:ring-border-focus-ring"
        />
        </TextField>
        <Button type="submit" leadingIcon={RiSendPlaneLine} disabled={!canSend}>
          {sending ? t("thread.composer.sending") : t("thread.composer.send")}
        </Button>
      </div>
    </form>
  );
}
