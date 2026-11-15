"use client";

import { useEffect, useState } from "react";
import {
  RiAddLine,
  RiChat1Line,
  RiKanbanView,
  RiMoreFill,
} from "@remixicon/react";

import { Button } from "@/components/base/buttons/button";
import {
  boardsGet,
  botsList,
  ticketsComment,
  ticketsCreate,
  ticketsMove,
  type TicketView,
  type RosterBotView,
} from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

type Phase = "loading" | "ready" | "absent";

const COLUMNS = ["backlog", "todo", "in_progress", "done"] as const;
type Column = (typeof COLUMNS)[number];

function parseColumn(state: string): Column {
  return COLUMNS.includes(state as Column) ? (state as Column) : "backlog";
}

/**
 * `/app/board` (spec/08-ui-ux.md §8.6, REQ-038).
 *
 * Flag-gated at the deployment: the screen probes `boards.get`; when the board surface is
 * absent, it renders the not-found shape — the route is absent, not a locked screen (REQ-038).
 */
export function BoardScreen() {
  const t = useTranslation();
  const [phase, setPhase] = useState<Phase>("loading");
  const [tickets, setTickets] = useState<TicketView[]>([]);
  const [bots, setBots] = useState<RosterBotView[]>([]);
  const [selected, setSelected] = useState<TicketView | null>(null);
  const [creating, setCreating] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [assignee, setAssignee] = useState("");
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [comment, setComment] = useState("");
  const [commenting, setCommenting] = useState(false);

  const load = () =>
    Promise.allSettled([boardsGet(), botsList()]).then(([boardResult, botsResult]) => {
      if (boardResult.status === "rejected") {
        const status = (boardResult.reason as { status?: number } | null)?.status;
        setPhase(status === 404 ? "absent" : "ready");
        setTickets([]);
      } else {
        setTickets(boardResult.value.tickets);
        setPhase("ready");
      }
      if (botsResult.status === "fulfilled") {
        setBots(botsResult.value.bots);
      }
    });

  useEffect(() => {
    const controller = new AbortController();
    Promise.allSettled([boardsGet(controller.signal), botsList(controller.signal)]).then(
      ([boardResult, botsResult]) => {
        if (controller.signal.aborted) {
          return;
        }
        if (boardResult.status === "rejected") {
          const status = (boardResult.reason as { status?: number } | null)?.status;
          setPhase(status === 404 ? "absent" : "ready");
          return;
        }
        setTickets(boardResult.value.tickets);
        setPhase("ready");
        if (botsResult.status === "fulfilled") {
          setBots(botsResult.value.bots);
        }
      },
    );
    return () => controller.abort();
  }, []);

  const botName = (id: string | null | undefined): string =>
    id ? (bots.find((entry) => entry.id === id)?.name ?? id) : "";

  const createTicket = async () => {
    const trimmed = title.trim();
    if (!trimmed) {
      setActionError(t("board.error.title"));
      return;
    }
    setBusy(true);
    setActionError(null);
    try {
      await ticketsCreate({
        title: trimmed,
        description: description.trim() || undefined,
        assigneeBotId: assignee || undefined,
      });
      setTitle("");
      setDescription("");
      setAssignee("");
      setCreating(false);
      await load();
    } catch {
      setActionError(t("board.error.create"));
    } finally {
      setBusy(false);
    }
  };

  const moveSelected = async (state: Column) => {
    if (!selected) {
      return;
    }
    setBusy(true);
    try {
      const moved = await ticketsMove({ ticketId: selected.id, state });
      setSelected(moved.ticket);
      await load();
    } catch {
      setActionError(t("board.error.move"));
    } finally {
      setBusy(false);
    }
  };

  const commentSelected = async () => {
    const body = comment.trim();
    if (!selected || !body) {
      return;
    }
    setCommenting(true);
    try {
      await ticketsComment({ ticketId: selected.id, body });
      setComment("");
      await load();
    } catch {
      setActionError(t("board.error.comment"));
    } finally {
      setCommenting(false);
    }
  };

  if (phase === "loading") {
    return (
      <main className="flex h-full flex-col gap-4 p-4 sm:p-6" aria-busy="true">
        <div className="bg-background-secondary-default h-8 w-48 rounded-md" />
        <div className="grid grid-cols-1 gap-4 md:grid-cols-4">
          {COLUMNS.map((column) => (
            <div key={column} className="bg-background-secondary-default h-64 rounded-2xl" />
          ))}
        </div>
      </main>
    );
  }

  if (phase === "absent") {
    // The route is absent when the deployment does not enable the board — not a locked screen.
    return (
      <main className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <RiKanbanView className="text-foreground-icon-tertiary h-8 w-8" aria-hidden="true" />
        <p className="text-text-primary text-body-regular">{t("board.absent")}</p>
      </main>
    );
  }

  return (
    <main className="flex h-full min-w-0 flex-col gap-4 overflow-y-auto p-4 sm:p-6">
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="text-title-3-semibold text-text-primary">{t("board.title")}</h1>
          <p className="text-text-secondary text-body-regular">{t("board.subtitle")}</p>
        </div>
        <Button variant="primary" onClick={() => setCreating((value) => !value)}>
          <RiAddLine className="h-4 w-4" aria-hidden="true" />
          {t("board.new-ticket")}
        </Button>
      </header>

      {creating ? (
        <form
          className="border-border-button-default bg-background-primary-default flex flex-col gap-3 rounded-2xl border p-4"
          onSubmit={(event) => {
            event.preventDefault();
            void createTicket();
          }}
        >
          <label className="flex flex-col gap-1">
            <span className="text-text-secondary text-body-2-medium">{t("board.field.title")}</span>
            <input
              autoFocus
              type="text"
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-10 rounded-md border px-3 text-body-regular placeholder:text-text-placeholder"
              placeholder={t("board.field.title-placeholder")}
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-text-secondary text-body-2-medium">{t("board.field.description")}</span>
            <textarea
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              rows={2}
              className="boma-focus text-text-primary bg-background-primary-default border-border-button-default rounded-md border px-3 py-2 text-body-regular placeholder:text-text-placeholder"
              placeholder={t("board.field.description-placeholder")}
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-text-secondary text-body-2-medium">{t("board.field.assignee")}</span>
            <select
              value={assignee}
              onChange={(event) => setAssignee(event.target.value)}
              className="boma-focus border-border-button-default bg-background-primary-default h-10 rounded-md border px-2 text-body-regular"
            >
              <option value="">{t("board.field.assignee-none")}</option>
              {bots.map((entry) => (
                <option key={entry.id} value={entry.id}>{entry.name}</option>
              ))}
            </select>
          </label>
          {actionError ? <p className="text-text-error-primary text-body-2-medium" role="alert">{actionError}</p> : null}
          <div className="flex items-center gap-2">
            <Button variant="primary" type="submit" disabled={busy}>
              {busy ? t("board.creating") : t("board.create")}
            </Button>
            <Button variant="ghost" type="button" onClick={() => setCreating(false)}>
              {t("common.cancel")}
            </Button>
          </div>
        </form>
      ) : null}

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-4">
        {COLUMNS.map((column) => {
          const columnTickets = tickets.filter((ticket) => parseColumn(ticket.state) === column);
          return (
            <section key={column} aria-label={t(`board.column.${column}`)} className="flex flex-col gap-2">
              <h2 className="text-text-secondary text-body-2-semibold uppercase tracking-wide text-caption-1-semibold">
                {t(`board.column.${column}`)}
                <span className="text-text-tertiary ml-2">{columnTickets.length}</span>
              </h2>
              <ul className="flex min-h-32 flex-col gap-2">
                {columnTickets.map((ticket) => (
                  <li key={ticket.id}>
                    <button
                      type="button"
                      onClick={() => setSelected(ticket)}
                      className="boma-focus border-border-button-default bg-background-primary-default hover:bg-background-primary-hover flex w-full flex-col gap-1 rounded-2xl border px-3 py-2 text-start transition-colors"
                    >
                      <span className="text-text-tertiary text-body-2-medium">{ticket.key}</span>
                      <span className="text-text-primary text-body-medium line-clamp-2">{ticket.title}</span>
                      <span className="flex items-center gap-2 text-text-tertiary text-body-2-medium">
                        {ticket.assigneeBotId ? (
                          <span className="bg-background-secondary-default inline-flex items-center gap-1 rounded-full px-2 py-0.5">
                            {botName(ticket.assigneeBotId)}
                          </span>
                        ) : null}
                        {ticket.commentCount > 0 ? (
                          <span className="inline-flex items-center gap-1">
                            <RiChat1Line className="h-4 w-4" aria-hidden="true" />
                            {ticket.commentCount}
                          </span>
                        ) : null}
                      </span>
                    </button>
                  </li>
                ))}
                {columnTickets.length === 0 ? (
                  <li className="text-text-tertiary text-body-2-medium border-border-separator rounded-2xl border border-dashed px-3 py-4 text-center">
                    {t("board.column-empty")}
                  </li>
                ) : null}
              </ul>
            </section>
          );
        })}
      </div>

      {selected ? (
        <div className="fixed inset-0 z-20 flex items-end justify-center md:items-center" role="dialog" aria-modal="true" aria-label={`${selected.key} ${selected.title}`}>
          <button
            type="button"
            className="absolute inset-0 h-full w-full cursor-default bg-black/40"
            aria-label={t("common.close")}
            onClick={() => setSelected(null)}
          />
          <div className="bg-background-primary-default border-border-button-default relative flex max-h-[80dvh] w-full max-w-lg flex-col gap-4 overflow-y-auto rounded-t-3xl border p-6 md:rounded-3xl">
            <div className="flex items-start justify-between gap-3">
              <div className="flex flex-col gap-1">
                <span className="text-text-tertiary text-body-2-medium">{selected.key} · {t(`board.column.${parseColumn(selected.state)}`)}</span>
                <h2 className="text-headline-medium text-text-primary break-words">{selected.title}</h2>
              </div>
              <button type="button" className="boma-target boma-focus text-text-tertiary hover:text-text-secondary rounded-md" aria-label={t("common.close")} onClick={() => setSelected(null)}>
                <RiMoreFill className="h-5 w-5" aria-hidden="true" />
              </button>
            </div>
            {selected.description ? <p className="text-text-secondary text-body-regular whitespace-pre-wrap">{selected.description}</p> : null}
            {selected.assigneeBotId ? (
              <p className="text-text-secondary text-body-2-medium">
                {t("board.assigned")} {botName(selected.assigneeBotId)}
              </p>
            ) : null}
            <div className="flex flex-wrap items-center gap-2">
              {COLUMNS.map((column) => (
                <Button
                  key={column}
                  variant={parseColumn(selected.state) === column ? "primary" : "secondary"}
                  onClick={() => void moveSelected(column)}
                  disabled={busy}
                >
                  {t(`board.column.${column}`)}
                </Button>
              ))}
            </div>
            <form
              className="border-separator-border flex flex-col gap-2 border-t pt-4"
              onSubmit={(event) => {
                event.preventDefault();
                void commentSelected();
              }}
            >
              <label className="flex flex-col gap-1">
                <span className="text-text-secondary text-body-2-medium">{t("board.comment")}</span>
                <textarea
                  value={comment}
                  onChange={(event) => setComment(event.target.value)}
                  rows={2}
                  className="boma-focus text-text-primary bg-background-primary-default border-border-button-default rounded-md border px-3 py-2 text-body-regular placeholder:text-text-placeholder"
                  placeholder={t("board.comment-placeholder")}
                />
              </label>
              {actionError ? <p className="text-text-error-primary text-body-2-medium" role="alert">{actionError}</p> : null}
              <Button variant="secondary" type="submit" disabled={commenting || !comment.trim()}>
                {commenting ? t("board.commenting") : t("board.comment-submit")}
              </Button>
            </form>
          </div>
        </div>
      ) : null}
    </main>
  );
}
