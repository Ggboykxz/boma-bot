"use client";

import { RiArrowRightLine } from "@remixicon/react";

import { ButtonLink } from "@/components/base/buttons/button";
import { useTranslation } from "@/lib/use-translation";

/**
 * The root screen (spec/08-ui-ux.md §8.1).
 *
 * Sign-in redirects to `/app` once sessions exist (M1); at M0 this page is a static
 * landmark that the shell and the axe suites can rely on.
 */
export function LandingScreen() {
  const t = useTranslation();

  return (
    <main className="mx-auto flex min-h-dvh max-w-2xl flex-col justify-center gap-6 px-6 py-16">
      <p className="text-text-secondary text-body-strong">{t("app.name")}</p>
      <h1 className="text-title-2-medium text-text-primary">{t("landing.title")}</h1>
      <p className="text-text-secondary text-body-medium">{t("landing.body")}</p>
      <p>
        <ButtonLink href="/app/" trailingIcon={RiArrowRightLine}>
          {t("landing.cta")}
        </ButtonLink>
      </p>
    </main>
  );
}
