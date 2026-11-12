"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import type { ReactNode } from "react";

import { BotRail } from "@/components/shell/bot-rail";
import { useTranslation } from "@/lib/use-translation";

const NAV = [
  { href: "/app/", key: "shell.nav.threads" },
  { href: "/app/", key: "shell.nav.settings" },
] as const;

export interface AppShellProps {
  /** The screen itself. */
  children?: ReactNode;
}

/**
 * The application shell (spec/08-ui-ux.md §8.1, §8.3).
 *
 * The landmarks are the contract the UI goldens assert on; the seeded screens replace the
 * placeholder inside `main` from M1. Every label comes from the catalogue, and every colour
 * is a BoardUI semantic token, so the shell is already dark-mode and RTL correct.
 *
 * The navigation labels use the primary text token: on the secondary surface the secondary
 * text token reaches 4.42:1 in light mode, under the 4.5:1 the axe gate demands.
 *
 * The left rail carries the bots and the activity entry. The rail is a set of links and not a
 * list: the thread and the activity screen each own one list, and a second list here would make
 * the counts the goldens assert ambiguous.
 */
export function AppShell({ children }: AppShellProps) {
  const t = useTranslation();
  const pathname = usePathname();

  return (
    <div className="flex min-h-dvh flex-col">
      <header className="bg-background-secondary-default border-separator-border border-b">
        <nav
          aria-label={t("shell.nav.primary")}
          className="mx-auto flex w-full max-w-5xl items-center gap-1 px-4 py-2"
        >
          {NAV.map((item) => (
            <Link
              key={item.key}
              href={item.href}
              aria-current={pathname === item.href ? "page" : undefined}
              className="boma-target boma-focus text-text-primary hover:bg-background-secondary-hover inline-flex items-center rounded-md px-3 text-body-medium"
            >
              {t(item.key)}
            </Link>
          ))}
        </nav>
      </header>
      <div className="mx-auto flex w-full max-w-5xl flex-1 flex-col gap-6 px-4 py-6 lg:flex-row lg:gap-8">
        <BotRail />
        <main className="flex min-w-0 flex-1 flex-col gap-3">
          {children}
        </main>
      </div>
    </div>
  );
}
