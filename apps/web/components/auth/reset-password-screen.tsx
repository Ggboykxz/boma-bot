"use client";

import Link from "next/link";
import { RiErrorWarningLine } from "@remixicon/react";
import { useRouter, useSearchParams } from "next/navigation";
import { useCallback, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import { AuthShell } from "@/components/auth/auth-shell";
import { authResetPassword } from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

const MIN_PASSWORD_LENGTH = 10;

/**
 * `/reset-password?token=…` (spec/08-ui-ux.md §8.2, REQ-040).
 *
 * The token is validated by the server as opaque; this screen does the same checks locally that
 * the server does, refuses obviously invalid passwords, and signs the person in on success.
 */
export function ResetPasswordScreen() {
  const t = useTranslation();
  const searchParams = useSearchParams();
  const router = useRouter();
  const token = searchParams.get("token") ?? "";

  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [status, setStatus] = useState<"idle" | "submitting" | "succeeded" | "failed">("idle");
  const [error, setError] = useState<string | null>(null);

  const localValidation = useCallback((): string | null => {
    if (password.length < MIN_PASSWORD_LENGTH) {
      return t("reset.error.short");
    }
    if (password !== confirm) {
      return t("reset.error.mismatch");
    }
    return null;
  }, [password, confirm, t]);

  const submit = useCallback(
    async (event: React.FormEvent) => {
      event.preventDefault();
      const local = localValidation();
      if (local) {
        setError(local);
        return;
      }
      setStatus("submitting");
      setError(null);
      try {
        await authResetPassword({ token, password });
        setStatus("succeeded");
        router.replace("/app/");
      } catch {
        setStatus("failed");
        setError(t("reset.error.invalid"));
      }
    },
    [token, password, localValidation, router, t],
  );

  if (!token) {
    return (
      <AuthShell title={t("reset.title")}>
        <p className="text-text-primary text-body-regular">{t("reset.missing-token")}</p>
        <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary" href="/forgot-password">
          {t("forgot.submit")}
        </Link>
      </AuthShell>
    );
  }

  return (
    <AuthShell title={t("reset.title")} subtitle={t("reset.subtitle")}>
      <form onSubmit={submit} className="flex flex-col gap-3" aria-busy={status === "submitting"}>
        <label className="flex flex-col gap-1">
          <span className="text-text-secondary text-body-2-medium">{t("reset.password")}</span>
          <input
            type="password"
            required
            autoComplete="new-password"
            minLength={MIN_PASSWORD_LENGTH}
            className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-10 rounded-md border px-3 text-body-regular"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            disabled={status === "submitting"}
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-text-secondary text-body-2-medium">{t("reset.confirm")}</span>
          <input
            type="password"
            required
            autoComplete="new-password"
            minLength={MIN_PASSWORD_LENGTH}
            className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-10 rounded-md border px-3 text-body-regular"
            value={confirm}
            onChange={(event) => setConfirm(event.target.value)}
            disabled={status === "submitting"}
          />
        </label>
        {error ? <p className="text-text-error-primary text-body-2-medium" role="alert">{error}</p> : null}
        <Button variant="primary" size="medium" type="submit" disabled={status === "submitting"}>
          {status === "submitting" ? t("reset.submitting") : t("reset.submit")}
        </Button>
      </form>
      <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary text-body-2-medium" href="/sign-in">
        {t("forgot.back")}
      </Link>
    </AuthShell>
  );
}
