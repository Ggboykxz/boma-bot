"use client";

import Link from "next/link";
import { RiErrorWarningLine } from "@remixicon/react";
import { useRouter, useSearchParams } from "next/navigation";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import { AuthShell } from "@/components/auth/auth-shell";
import {
  authCapabilities,
  authSignIn,
  type AuthCapabilities,
} from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

type Phase = "loading" | "ready" | "error";
type Status = "idle" | "submitting" | "failed";

/**
 * `/sign-in` (spec/08-ui-ux.md §8.2, REQ-040, REQ-041).
 *
 * The methods shown come from `/api/auth/capabilities`: password when it exists, passkey when the
 * deployment reports one, SSO per configured provider, e-mail code likewise. Hidden methods are
 * absent, not disabled. A wrong credential never says whether the address exists.
 */
export function SignInScreen() {
  const t = useTranslation();
  const searchParams = useSearchParams();
  const router = useRouter();
  const next = searchParams.get("next") ?? "/app/";

  const [phase, setPhase] = useState<Phase>("loading");
  const [reloadToken, setReloadToken] = useState(0);
  const [capabilities, setCapabilities] = useState<AuthCapabilities | null>(null);
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
        await authSignIn({ email: email.trim(), password });
        router.replace(next);
      } catch (error: unknown) {
        setStatus("failed");
        // The server never says whether the address exists; the screen says the same thing.
        setFieldError(t("signin.error.credentials"));
      }
    },
    [email, password, next, router, t],
  );

  if (phase === "loading") {
    return (
      <AuthShell title={t("signin.title")} subtitle={t("signin.subtitle")}>
        <div aria-hidden="true" className="flex flex-col gap-3">
          <div className="bg-background-secondary-default h-10 w-full rounded-md" />
          <div className="bg-background-secondary-default h-10 w-full rounded-md" />
          <div className="bg-background-secondary-default h-10 w-24 rounded-md" />
        </div>
      </AuthShell>
    );
  }

  if (phase === "error" || !capabilities) {
    return (
      <AuthShell title={t("signin.title")}>
        <div className="flex items-start gap-2">
          <RiErrorWarningLine className="text-foreground-icon-primary mt-0.5 h-5 w-5" aria-hidden="true" />
          <div className="flex flex-col gap-2">
            <p className="text-text-primary text-body-regular">{t("signin.error.unavailable")}</p>
            <Button variant="secondary" onClick={load}>{t("common.retry")}</Button>
          </div>
        </div>
      </AuthShell>
    );
  }

  const busy = status === "submitting";
  return (
    <AuthShell title={t("signin.title")} subtitle={t("signin.subtitle")}>
      {capabilities.password ? (
        <form onSubmit={submit} className="flex flex-col gap-3" aria-busy={busy}>
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
              autoComplete="current-password"
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
            {busy ? t("signin.submitting") : t("signin.submit")}
          </Button>
        </form>
      ) : (
        <p className="text-text-tertiary text-body-regular">{t("signin.no-password")}</p>
      )}

      <div className="border-separator-border flex flex-col gap-2 border-t pt-4 text-body-2-medium">
        {capabilities.passkey ? (
          <p className="text-text-tertiary">{t("signin.passkey-unavailable")}</p>
        ) : null}
        {capabilities.sso.map((provider) => (
          <a
            key={provider}
            className="boma-target boma-focus text-text-primary hover:text-text-secondary inline-flex items-center gap-2 rounded-md"
            href={`/api/auth/${provider}`}
          >
            {t("signin.sso")} {provider}
          </a>
        ))}
        <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary" href="/forgot-password">
          {t("signin.forgot")}
        </Link>
        {capabilities.signup === "open" ? (
          <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary" href={`/sign-up?next=${encodeURIComponent(next)}`}>
            {t("signin.create-account")}
          </Link>
        ) : null}
      </div>
    </AuthShell>
  );
}
