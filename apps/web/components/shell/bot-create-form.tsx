"use client";

import { RiAddLine } from "@remixicon/react";
import { useRouter } from "next/navigation";
import { useState } from "react";

import { Button } from "@/components/base/buttons/button";
import { IconButton } from "@/components/base/buttons/icon-button";
import { Input } from "@/components/base/input/input";
import { botsCreate } from "@/lib/api/rpc";
import { refreshRoster } from "@/lib/roster";
import { useTranslation } from "@/lib/use-translation";

/**
 * Create another teammate after onboarding (spec/01 REQ-004, spec/08 §8.3).
 *
 * The rail used to offer this action for groups only. The form mirrors that inline pattern with
 * BoardUI field components: name is required, description is optional, and a failed create keeps
 * the form open with a catalogue error instead of taking the keyboard away.
 */
export function BotCreateForm() {
  const t = useTranslation();
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [failed, setFailed] = useState(false);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const trimmed = name.trim();
    if (trimmed.length === 0 || submitting) {
      return;
    }
    setSubmitting(true);
    setFailed(false);
    try {
      const bot = await botsCreate({
        name: trimmed,
        ...(description.trim().length > 0 ? { description: description.trim() } : {}),
      });
      await refreshRoster();
      setOpen(false);
      setName("");
      setDescription("");
      router.push(`/app/${bot.id}/`);
    } catch {
      setFailed(true);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center justify-between gap-1">
        <h2 className="text-text-secondary text-caption-1-semibold">{t("shell.rail.bots")}</h2>
        <IconButton
          icon={RiAddLine}
          size="small"
          aria-label={t("shell.rail.new-bot")}
          aria-expanded={open}
          onClick={() => setOpen((value) => !value)}
        />
      </div>
      {open ? (
        <form
          onSubmit={(event) => {
            void submit(event);
          }}
          className="border-border-button-default flex flex-col gap-2 rounded-md border p-2"
        >
          <Input
            label={t("bots.create.name")}
            value={name}
            onChange={setName}
            placeholder={t("bots.create.name.placeholder")}
            size="small"
            validationBehavior="aria"
            isInvalid={name.trim().length === 0 && failed}
            errorMessage={t("bots.create.name")}
          />
          <Input
            label={t("bots.create.description")}
            value={description}
            onChange={setDescription}
            size="small"
          />
          {failed ? (
            <p role="alert" className="text-text-error-primary text-body-2-medium">
              {t("bots.create.error")}
            </p>
          ) : null}
          <Button type="submit" size="small" disabled={submitting || name.trim().length === 0}>
            {t("bots.create.submit")}
          </Button>
        </form>
      ) : null}
    </div>
  );
}
