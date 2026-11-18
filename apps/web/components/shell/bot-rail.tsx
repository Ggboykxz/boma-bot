"use client";

import { RiAddLine, RiArrowDownSLine, RiArrowUpSLine, RiTimeLine } from "@remixicon/react";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { useEffect, useState, useSyncExternalStore } from "react";

import { listBots, serverBots, subscribeBots } from "@/lib/bot-directory";
import { BotCreateForm } from "@/components/shell/bot-create-form";
import {
  botsList,
  groupsCreate,
  groupsList,
  threadsUnsnooze,
  type GroupListView,
  type RosterBotView,
} from "@/lib/api/rpc";
import { listRoster, refreshRoster, subscribeRoster, serverRoster } from "@/lib/roster";
import { useTranslation } from "@/lib/use-translation";

/** How the rail renders one bot, and how it unsnoozes it. */
function RosterRow({
  bot,
  current,
  onUnsnooze,
}: {
  bot: RosterBotView;
  current: boolean;
  onUnsnooze: (threadId: string) => void;
}) {
  const t = useTranslation();

  return (
    <div className="animate-row-slide-in flex items-center gap-1">
      <Link
        href={`/app/${bot.id}/`}
        aria-current={current ? "page" : undefined}
        className="boma-target boma-focus text-text-primary hover:bg-background-secondary-hover inline-flex flex-1 items-center justify-between gap-2 rounded-md px-2 text-body-medium"
      >
        <span className="truncate">{bot.name}</span>
        {bot.unreadCount > 0 ? (
          <span
            key={bot.unreadCount}
            className="animate-number-fade bg-background-tertiary-default text-text-primary rounded-full px-2 text-caption-1-semibold"
          >
            {bot.unreadCount}
          </span>
        ) : null}
      </Link>
      {bot.snoozedUntil ? (
        <button
          type="button"
          onClick={() => onUnsnooze(bot.threadId)}
          aria-label={t("shell.rail.unsnooze")}
          className="boma-focus text-text-tertiary hover:text-text-primary rounded-md p-1"
        >
          <RiTimeLine size={14} aria-hidden />
        </button>
      ) : null}
    </div>
  );
}

/**
 * The left rail (spec/08-ui-ux.md §8.3).
 *
 * The bots and their per-member state come from the server roster: a bot snoozed by the caller
 * leaves the regular list while the deadline is in the future, shows up on the snoozed shelf, and
 * returns on its own. Until the first answer arrives the rail shows the bots the client has
 * already met; a server that cannot answer keeps that fallback, it never breaks the screen.
 */
export function BotRail() {
  const t = useTranslation();
  const pathname = usePathname();
  const [expanded, setExpanded] = useState(false);
  const [groups, setGroups] = useState<GroupListView[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [bots, setBots] = useState<RosterBotView[] | null>(null);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [submitting, setSubmitting] = useState(false);
  const [createFailed, setCreateFailed] = useState(false);
  const router = useRouter();
  const roster = useSyncExternalStore(subscribeRoster, listRoster, serverRoster);
  const directory = useSyncExternalStore(subscribeBots, listBots, serverBots);

  useEffect(() => {
    void refreshRoster();
    const controller = new AbortController();
    groupsList(controller.signal)
      .then((body) => setGroups(body.groups))
      .catch(() => setGroups([]));
    return () => controller.abort();
  }, [pathname]);

  useEffect(() => {
    if (!creating || bots !== null) {
      return;
    }
    const controller = new AbortController();
    botsList(controller.signal)
      .then((reply) => setBots(reply.bots.filter((bot) => !bot.archivedAt)))
      .catch(() => setBots([]));
    return () => controller.abort();
  }, [creating, bots]);

  const toggleBot = (id: string) => {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const createGroup = async (event: React.FormEvent) => {
    event.preventDefault();
    const trimmed = name.trim();
    if (trimmed.length === 0 || selected.size === 0 || submitting) {
      return;
    }
    setSubmitting(true);
    setCreateFailed(false);
    try {
      const reply = await groupsCreate({ name: trimmed, botIds: [...selected] });
      setGroups((current) => [
        ...current,
        {
          id: reply.id,
          name: reply.name,
          threadId: reply.threadId,
          ownerBotId: reply.memberBotIds[0] ?? "",
          memberCount: reply.memberBotIds.length,
          createdAt: "",
        },
      ]);
      setCreating(false);
      setName("");
      setSelected(new Set());
      router.push(`/app/g/${reply.id}/`);
    } catch {
      setCreateFailed(true);
    } finally {
      setSubmitting(false);
    }
  };

  const unsnooze = async (threadId: string) => {
    try {
      await threadsUnsnooze({ threadId });
    } catch {
      // The shelf keeps the bot; the refresh decides.
    }
    await refreshRoster();
  };

  const regular = roster === null ? [] : roster.filter((bot) => !bot.snoozedUntil);
  const snoozed = roster === null ? [] : roster.filter((bot) => bot.snoozedUntil);

  return (
    <aside className="flex w-full flex-col gap-6 lg:w-56 lg:shrink-0">
      <nav aria-label={t("shell.rail.bots")} className="flex flex-col gap-1">
        <BotCreateForm />
        {roster === null && directory.length === 0 ? (
          <p className="text-text-secondary text-body-2-regular">{t("shell.rail.bots.empty")}</p>
        ) : null}
        <div className="flex flex-col gap-1">
          {roster === null
            ? directory.map((bot) => (
                <div key={bot.id}>
                  <Link
                    href={`/app/${bot.id}/`}
                    aria-current={pathname === `/app/${bot.id}/` ? "page" : undefined}
                    className="boma-target boma-focus text-text-primary hover:bg-background-secondary-hover inline-flex items-center rounded-md px-2 text-body-medium"
                  >
                    {bot.name}
                  </Link>
                </div>
              ))
            : regular.map((bot) => (
                <RosterRow
                  key={bot.id}
                  bot={bot}
                  current={pathname === `/app/${bot.id}/`}
                  onUnsnooze={(threadId) => {
                    void unsnooze(threadId);
                  }}
                />
              ))}
        </div>
        {snoozed.length > 0 ? (
          <div className="flex flex-col gap-1">
            <button
              type="button"
              aria-expanded={expanded}
              onClick={() => setExpanded((value) => !value)}
              className="boma-focus text-text-secondary hover:text-text-primary inline-flex w-fit items-center gap-1 rounded-md px-1 text-body-2-medium"
            >
              {expanded ? (
                <RiArrowUpSLine size={16} aria-hidden />
              ) : (
                <RiArrowDownSLine size={16} aria-hidden />
              )}
              {t("shell.rail.snoozed")} ({snoozed.length})
            </button>
            {expanded ? (
              <div className="flex flex-col gap-1 ps-2">
                {snoozed.map((bot) => (
                  <RosterRow
                    key={bot.id}
                    bot={bot}
                    current={pathname === `/app/${bot.id}/`}
                    onUnsnooze={(threadId) => {
                      void unsnooze(threadId);
                    }}
                  />
                ))}
              </div>
            ) : null}
          </div>
        ) : null}
      </nav>
      <nav aria-label={t("shell.rail.groups")} className="flex flex-col gap-1">
        <div className="flex items-center justify-between gap-1">
          <h2 className="text-text-secondary text-caption-1-semibold">{t("shell.rail.groups")}</h2>
          <button
            type="button"
            onClick={() => setCreating((value) => !value)}
            aria-expanded={creating}
            aria-label={t("shell.rail.new-group")}
            className="boma-focus text-text-tertiary hover:text-text-primary rounded-md p-1"
          >
            <RiAddLine size={14} aria-hidden />
          </button>
        </div>
        {creating ? (
          <form
            onSubmit={(event) => {
              void createGroup(event);
            }}
            className="border-border-button-default flex flex-col gap-2 rounded-md border p-2"
          >
            <label
              htmlFor="group-name"
              className="text-text-secondary text-caption-1-semibold"
            >
              {t("groups.create.name")}
            </label>
            <input
              id="group-name"
              type="text"
              autoFocus
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={t("groups.create.name.placeholder")}
              className="border-border-button-default placeholder:text-text-placeholder text-text-primary rounded-md border px-2 py-1 text-body-medium outline-none"
            />
            <fieldset className="flex flex-col gap-0.5">
              <legend className="text-text-secondary text-caption-1-semibold">
                {t("groups.create.bots")}
              </legend>
              {bots === null ? (
                <p className="text-text-tertiary text-body-2-regular">{t("thread.loading")}</p>
              ) : bots.length === 0 ? (
                <p className="text-text-tertiary text-body-2-regular">{t("groups.create.no-bots")}</p>
              ) : (
                bots.map((bot) => (
                  <label
                    key={bot.id}
                    className="text-text-primary flex items-center gap-1.5 rounded-md px-1 text-body-2-regular"
                  >
                    <input
                      type="checkbox"
                      checked={selected.has(bot.id)}
                      onChange={() => toggleBot(bot.id)}
                    />
                    <span className="truncate">{bot.name}</span>
                  </label>
                ))
              )}
            </fieldset>
            {createFailed ? (
              <p role="alert" className="text-text-error-primary text-body-2-medium">
                {t("groups.create.error")}
              </p>
            ) : null}
            <button
              type="submit"
              disabled={submitting || selected.size === 0 || name.trim().length === 0}
              className="boma-focus bg-background-tertiary-default text-text-primary hover:bg-background-tertiary-hover disabled:opacity-50 rounded-md px-2 py-1 text-body-2-medium"
            >
              {t("groups.create.submit")}
            </button>
          </form>
        ) : null}
        <div className="flex flex-col gap-1">
            {groups.map((group) => {
              const href = `/app/g/${group.id}/`;
              return (
                <Link
                  key={group.id}
                  href={href}
                  aria-current={pathname === href ? "page" : undefined}
                  className="boma-target boma-focus text-text-primary hover:bg-background-secondary-hover inline-flex items-center justify-between gap-2 rounded-md px-2 text-body-medium"
                >
                  <span className="truncate">{group.name}</span>
                  <span className="text-text-tertiary text-caption-1-semibold">{group.memberCount}</span>
                </Link>
              );
            })}
        </div>
      </nav>
      <nav aria-label={t("shell.rail.primary")} className="flex flex-col gap-1">
        <Link
          href="/app/activity/"
          aria-current={pathname?.startsWith("/app/activity") ? "page" : undefined}
          className="boma-target boma-focus text-text-primary hover:bg-background-secondary-hover inline-flex items-center rounded-md px-2 text-body-medium"
        >
          {t("shell.rail.activity")}
        </Link>
      </nav>
    </aside>
  );
}
