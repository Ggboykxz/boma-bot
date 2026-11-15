"use client";

import Link from "next/link";
import { RiErrorWarningLine } from "@remixicon/react";
import { useRouter, useSearchParams } from "next/navigation";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import { AuthShell } from "@/components/auth/auth-shell";
import { authCapabilities, authSignUp, type AuthCapabilities } from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

type Phase = "loading" | "ready" | "error";
type Status = "idle" | "submitting" | "failed";

/**
 * `/sign-up` (spec/08-ui-ux.md §8.2, REQ-040).
 *
 * The first account on a fresh deployment may become the operator: the screen says so instead of
 * silently minting power. A closed `/sign-up` says so plainly instead of hiding a form that always
 * fails.
 */
export function SignUpScreen() {
  const t = useTranslation();
  const searchParams = useSearchParams();
  const router = useRouter();
  // Only same-origin paths are accepted; an absolute or protocol-relative URL in the query
  // string would otherwise bounce the freshly authenticated member to an arbitrary origin.
  const rawNext = searchParams.get("next");
  const next = rawNext && rawNext.startsWith("/") && !rawNext.startsWith("//") ? rawNext : "/onboarding/1/";

  const [phase, setPhase] = useState<Phase>("loading");
  const [reloadToken, setReloadToken] = useState(0);
  const [capabilities, setCapabilities] = useState<AuthCapabilities | null>(null);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [status, setStatus] = useState<Status>("idle");
  const [fieldError, setFieldError] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    authCapabilities(controller.signal)
      .then((body) => {
        if (controller.signal.aborted) {
          return;
        }
        setCapabilities(body);
        setPhase("ready");
      })
      .catch(() => {
        if (!controller.signal.aborted) {
          setPhase("error");
        }
      });
    return () => controller.abort();
  }, [reloadToken]);

  const load = useCallback(() => {
    setPhase("loading");
    setReloadToken((token) => token + 1);
  }, []);

  const submit = useCallback(
    async (event: React.FormEvent) => {
      event.preventDefault();
      setStatus("submitting");
      setFieldError(null);
      try {
        await authSignUp({ email: email.trim(), password, name: name.trim(), locale: "en" });
        router.replace(next);
      } catch {
        setStatus("failed");
        setFieldError(t("signup.error"));
      }
    },
    [email, password, name, next, router, t],
  );

  if (phase === "loading") {
    return (
      <AuthShell title={t("signup.title")}>
        <div aria-hidden="true" className="flex flex-col gap-3">
          <div className="bg-background-secondary-default h-10 w-full rounded-md" />
          <div className="bg-background-secondary-default h-10 w-full rounded-md" />
          <div className="bg-background-secondary-default h-10 w-full rounded-md" />
          <div className="bg-background-secondary-default h-10 w-32 rounded-md" />
        </div>
      </AuthShell>
    );
  }

  if (phase === "error" || !capabilities) {
    return (
      <AuthShell title={t("signup.title")}>
        <div className="flex items-start gap-2">
          <RiErrorWarningLine className="text-foreground-icon-primary mt-0.5 h-5 w-5" aria-hidden="true" />
          <div className="flex flex-col gap-2">
            <p className="text-text-primary text-body-regular">{t("signup.error.unavailable")}</p>
            <Button variant="secondary" onClick={load}>{t("common.retry")}</Button>
          </div>
        </div>
      </AuthShell>
    );
  }

  if (capabilities.signup !== "open") {
    return (
      <AuthShell title={t("signup.title")}>
        <p className="text-text-primary text-body-regular">{t("signup.closed")}</p>
        <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary" href="/sign-in">
          {t("signin.submit")}
        </Link>
      </AuthShell>
    );
  }

  const busy = status === "submitting";
  return (
    <AuthShell title={t("signup.title")} subtitle={t("signup.subtitle")}>
      <p className="text-text-tertiary text-body-2-medium rounded-xl border border-separator-border px-3 py-2">
        {t("signup.first-account")}
      </p>
      <form onSubmit={submit} className="flex flex-col gap-3" aria-busy={busy}>
        <label className="flex flex-col gap-1">
          <span className="text-text-secondary text-body-2-medium">{t("signup.name")}</span>
          <input
            type="text"
            autoComplete="name"
            className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-10 rounded-md border px-3 text-body-regular"
            value={name}
            onChange={(event) => setName(event.target.value)}
            disabled={busy}
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-text-secondary text-body-2-medium">{t("signin.email")}</span>
          <input
            type="email"
            required
            autoComplete="email"
            spellCheck="false"
            className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-10 rounded-md border px-3 text-body-regular placeholder:text-text-placeholder"
            placeholder="person@example.com"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            disabled={busy}
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-text-secondary text-body-2-medium">{t("signin.password")}</span>
          <input
            type="password"
            required
            autoComplete="new-password"
            className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-10 rounded-md border px-3 text-body-regular"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            disabled={busy}
          />
        </label>
        {fieldError ? (
          <p className="text-text-error-primary text-body-2-medium" role="alert">{fieldError}</p>
        ) : null}
        <Button variant="primary" size="medium" type="submit" disabled={busy}>
          {busy ? t("signin.submitting") : t("signup.submit")}
        </Button>
      </form>
      <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary text-body-2-medium" href="/sign-in">
        {t("signup.have-account")}
      </Link>
    </AuthShell>
  );
}
