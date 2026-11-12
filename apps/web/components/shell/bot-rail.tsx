"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useSyncExternalStore } from "react";

import { listBots, serverBots, subscribeBots } from "@/lib/bot-directory";
import { useTranslation } from "@/lib/use-translation";

/**
 * The left rail (spec/08-ui-ux.md §8.3).
 *
 * Two regions: the bots the client knows, and the entry that opens the run-centric view. The
 * rail is a set of links, not a list: the thread screen and the activity screen each own one
 * list, and a second one here would make the counts the goldens assert ambiguous.
 *
 * The bot roster comes from the client, not from the server: the wire contract has no
 * `bots.list` procedure, so the rail shows the bots the session has already met.
 */
export function BotRail() {
  const t = useTranslation();
  const pathname = usePathname();
  const bots = useSyncExternalStore(subscribeBots, listBots, serverBots);

  return (
    <aside className="flex w-full flex-col gap-6 lg:w-56 lg:shrink-0">
      <nav aria-label={t("shell.rail.bots")} className="flex flex-col gap-1">
        <h2 className="text-text-secondary text-caption-1-semibold">{t("shell.rail.bots")}</h2>
        {bots.length === 0 ? (
          <p className="text-text-secondary text-body-2-regular">{t("shell.rail.bots.empty")}</p>
        ) : null}
        {bots.map((bot) => (
          <Link
            key={bot.id}
            href={`/app/${bot.id}/`}
            aria-current={pathname === `/app/${bot.id}/` ? "page" : undefined}
            className="boma-target boma-focus text-text-primary hover:bg-background-secondary-hover inline-flex items-center rounded-md px-2 text-body-medium"
          >
            {bot.name}
          </Link>
        ))}
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
