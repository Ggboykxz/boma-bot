"use client";

import Link from "next/link";

import { useTranslation } from "@/lib/use-translation";

const NAV = [
  { href: "/app/", key: "shell.nav.threads", current: true },
  { href: "/app/", key: "shell.nav.activity", current: false },
  { href: "/app/", key: "shell.nav.settings", current: false },
] as const;

/**
 * The application shell (spec/08-ui-ux.md §8.1).
 *
 * The landmarks are the contract the UI goldens assert on; the seeded screens replace the
 * placeholder inside `main` from M1. Every label comes from the catalogue, and every colour
 * is a BoardUI semantic token, so the shell is already dark-mode and RTL correct.
 *
 * The navigation labels use the primary text token: on the secondary surface the secondary
 * text token reaches 4.42:1 in light mode, under the 4.5:1 the axe gate demands.
 */
export function AppShell() {
  const t = useTranslation();

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
              aria-current={item.current ? "page" : undefined}
              className="boma-target boma-focus text-text-primary hover:bg-background-secondary-hover inline-flex items-center rounded-md px-3 text-body-medium"
            >
              {t(item.key)}
            </Link>
          ))}
        </nav>
      </header>
      <main className="mx-auto flex w-full max-w-5xl flex-1 flex-col gap-3 px-4 py-10">
        <h1 className="text-headline-medium text-text-primary">{t("shell.nav.threads")}</h1>
        <p className="text-text-secondary max-w-prose">{t("shell.placeholder")}</p>
      </main>
    </div>
  );
}
