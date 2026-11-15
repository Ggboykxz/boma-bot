"use client";

import Link from "next/link";
import { RiErrorWarningLine } from "@remixicon/react";
import { useRouter } from "next/navigation";
import { useCallback, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import { AuthShell } from "@/components/auth/auth-shell";
import { authForgotPassword } from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

/**
 * `/forgot-password` (spec/08-ui-ux.md §8.2, REQ-040).
 *
 * The response is the same either way: it never says whether the address is registered. The
 * screen states the outcome in the same words regardless of the answer.
 */
export function ForgotPasswordScreen() {
  const t = useTranslation();
  const router = useRouter();

  const [email, setEmail] = useState("");
  const [status, setStatus] = useState<"idle" | "submitting" | "sent" | "failed">("idle");
  const [error, setError] = useState<string | null>(null);

  const submit = useCallback(
    async (event: React.FormEvent) => {
      event.preventDefault();
      setStatus("submitting");
      setError(null);
      try {
        await authForgotPassword({ email: email.trim() });
        setStatus("sent");
      } catch {
        setStatus("failed");
        setError(t("forgot.error.generic"));
      }
    },
    [email, t],
  );

  return (
    <AuthShell title={t("forgot.title")} subtitle={t("forgot.subtitle")}>
      {status === "sent" ? (
        <div className="flex flex-col gap-3">
          <p className="text-text-primary text-body-regular">{t("forgot.sent")}</p>
          <Button variant="secondary" onClick={() => router.push("/sign-in")}>{t("forgot.back")}</Button>
        </div>
      ) : (
        <form onSubmit={submit} className="flex flex-col gap-3" aria-busy={status === "submitting"}>
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
              disabled={status === "submitting"}
            />
          </label>
          {error ? <p className="text-text-error-primary text-body-2-medium" role="alert">{error}</p> : null}
          <Button variant="primary" size="medium" type="submit" disabled={status === "submitting"}>
            {status === "submitting" ? t("forgot.submitting") : t("forgot.submit")}
          </Button>
        </form>
      )}
      <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary text-body-2-medium" href="/sign-in">
        {t("forgot.back")}
      </Link>
    </AuthShell>
  );
}
