"use client";

import { RiArrowDownSLine, RiArrowUpSLine, RiTimeLine } from "@remixicon/react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useCallback, useEffect, useState, useSyncExternalStore } from "react";

import { listBots, serverBots, subscribeBots } from "@/lib/bot-directory";
import { botsList, threadsUnsnooze, type RosterBotView } from "@/lib/api/rpc";
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
    <div className="flex items-center gap-1">
      <Link
        href={`/app/${bot.id}/`}
        aria-current={current ? "page" : undefined}
        className="boma-target boma-focus text-text-primary hover:bg-background-secondary-hover inline-flex flex-1 items-center justify-between gap-2 rounded-md px-2 text-body-medium"
      >
        <span className="truncate">{bot.name}</span>
        {bot.unreadCount > 0 ? (
          <span className="bg-background-tertiary-default text-text-secondary rounded-full px-2 text-caption-1-semibold">
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
  const [roster, setRoster] = useState<RosterBotView[] | null>(null);
  const [expanded, setExpanded] = useState(false);
  const directory = useSyncExternalStore(subscribeBots, listBots, serverBots);

  const loadRoster = useCallback(async () => {
    try {
      const reply = await botsList();
      setRoster(reply.bots);
    } catch {
      // The last good roster stays on screen; the fallback directory serves the first visit.
      setRoster((current) => current);
    }
  }, []);

  useEffect(() => {
    void loadRoster();
  }, [loadRoster, pathname]);

  useEffect(() => {
    const onRefresh = () => {
      void loadRoster();
    };
    window.addEventListener("boma:roster-refresh", onRefresh);
    return () => {
      window.removeEventListener("boma:roster-refresh", onRefresh);
    };
  }, [loadRoster]);

  const unsnooze = useCallback(
    async (threadId: string) => {
      try {
        await threadsUnsnooze({ threadId });
      } catch {
        // The shelf keeps the bot; the refresh decides.
      }
      window.dispatchEvent(new CustomEvent("boma:roster-refresh"));
    },
    [],
  );

  const regular =
    roster === null
      ? []
      : roster.filter((bot) => !bot.snoozedUntil);
  const snoozed =
    roster === null ? [] : roster.filter((bot) => bot.snoozedUntil);

  return (
    <aside className="flex w-full flex-col gap-6 lg:w-56 lg:shrink-0">
      <nav aria-label={t("shell.rail.bots")} className="flex flex-col gap-1">
        <h2 className="text-text-secondary text-caption-1-semibold">{t("shell.rail.bots")}</h2>
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
