"use client";

import { RiArrowRightLine } from "@remixicon/react";
import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";

import { ButtonLink } from "@/components/base/buttons/button";
import { botsList } from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

/**
 * The root screen (spec/08-ui-ux.md §8.1).
 *
 * Signed-in members go straight to `/app`; everyone else is sent to the sign-in screen. The
 * probe is one call: a signed-in client answers `200`, anything else lands on `/sign-in`.
 * The static landmark renders underneath until the answer arrives.
 */
export function LandingScreen() {
  const t = useTranslation();
  const router = useRouter();
  const [probeFailed, setProbeFailed] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    botsList(controller.signal)
      .then(() => {
        if (!controller.signal.aborted) {
          router.replace("/app/");
        }
      })
      .catch(() => {
        // Any answer that is not 200 — 401 in particular — sends the visitor to sign in.
        if (!controller.signal.aborted) {
          setProbeFailed(true);
          router.replace("/sign-in");
        }
      });
    return () => controller.abort();
    // The handler fires once, on mount; the router is stable for the lifetime of the page.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <main className="mx-auto flex min-h-dvh max-w-2xl flex-col justify-center gap-6 px-6 py-16">
      <p className="text-text-secondary text-body-strong">{t("app.name")}</p>
      <h1 className="text-title-2-medium text-text-primary">{t("landing.title")}</h1>
      <p className="text-text-secondary text-body-medium">{t("landing.body")}</p>
      <p>
        <ButtonLink href={probeFailed ? "/sign-in" : "/app/"} trailingIcon={RiArrowRightLine}>
          {t("landing.cta")}
        </ButtonLink>
      </p>
    </main>
  );
}
