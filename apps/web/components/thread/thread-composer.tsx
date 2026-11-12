"use client";

import { RiSendPlaneLine } from "@remixicon/react";
import { Label, TextArea, TextField } from "react-aria-components";

import { Button } from "@/components/base/buttons/button";
import { useTranslation } from "@/lib/use-translation";

export interface ThreadComposerProps {
  /** What the person wrote. */
  value: string;
  /** Called on every keystroke. */
  onChange: (value: string) => void;
  /** Called when the person sends. */
  onSend: () => void;
  /** Whether a send is in flight. */
  sending: boolean;
}

/**
 * The composer (spec/08-ui-ux.md §8.3).
 *
 * The text field is a real text field from React Aria, so it carries the `textbox` role, the
 * label and the keyboard behaviour a screen reader expects. The send button is disabled only
 * when there is nothing to send: a failed send never takes the keyboard away from the person.
 */
export function ThreadComposer({ value, onChange, onSend, sending }: ThreadComposerProps) {
  const t = useTranslation();
  const canSend = value.trim().length > 0 && !sending;

  return (
    <form
      className="border-border-button-default bg-background-primary-default flex items-end gap-2 rounded-3xl border p-2"
      onSubmit={(event) => {
        event.preventDefault();
        if (canSend) {
          onSend();
        }
      }}
    >
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
    </form>
  );
}
