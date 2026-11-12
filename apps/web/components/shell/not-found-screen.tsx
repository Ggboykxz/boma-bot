"use client";

import { ButtonLink } from "@/components/base/buttons/button";
import { useTranslation } from "@/lib/use-translation";

/** The `*` route of spec/08-ui-ux.md §8.1. One heading, one way back, no hint about what was not found. */
export function NotFoundScreen() {
  const t = useTranslation();

  return (
    <main className="mx-auto flex min-h-dvh max-w-xl flex-col justify-center gap-4 px-6 py-16">
      <h1 className="text-title-2-medium text-text-primary">{t("notfound.title")}</h1>
      <p className="text-text-secondary text-body-medium">{t("notfound.body")}</p>
      <p>
        <ButtonLink href="/" variant="secondary">
          {t("notfound.back")}
        </ButtonLink>
      </p>
    </main>
  );
}
