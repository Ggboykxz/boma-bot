"use client";

import { useTranslation } from "@/lib/use-translation";

/**
 * The threads home (spec/08-ui-ux.md §8.1).
 *
 * The shell's rail carries the bots; the home only says what to do next. The line comes from
 * the catalogue, like every other word of the product.
 */
export function ThreadsHome() {
  const t = useTranslation();

  return (
    <div className="flex flex-col gap-3">
      <h1 className="text-headline-medium text-text-primary">{t("shell.nav.threads")}</h1>
      <p className="text-text-secondary max-w-prose">{t("shell.placeholder")}</p>
    </div>
  );
}
