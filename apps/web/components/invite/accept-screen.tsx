"use client";

import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { RiErrorWarningLine } from "@remixicon/react";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import { AuthShell } from "@/components/auth/auth-shell";
import {
  invitationPreview,
  invitationsAccept,
  type InvitationPublicPreview,
} from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

type Phase = "loading" | "ready" | "unavailable" | "accepted";
type Status = "idle" | "submitting";

/**
 * `/invite/<token>` (spec/08-ui-ux.md §8.2, REQ-042).
 *
 * The page renders only what the token authorizes: the space's name, the inviter's display
 * name (when recorded), the role and the expiry. A used, expired or malformed token answers
 * the same "not available" sentence as any other credential that is not a valid invite. The
 * accept action goes to the RPC for the signed-in visitor; everyone else passes through
 * sign-in inline with `next` set to this page, so after auth they land back here.
 */
export function InviteScreen() {
  const t = useTranslation();
  const router = useRouter();
  const searchParams = useSearchParams();
  const token = searchParams.get("token") ?? "";

  const [phase, setPhase] = useState<Phase>("loading");
  const [preview, setPreview] = useState<InvitationPublicPreview | null>(null);
  const [status, setStatus] = useState<Status>("idle");
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    if (!token) {
      // Missing token entirely: the same not-available sentence, by a different road.
      Promise.resolve().then(() => {
        if (!controller.signal.aborted) {
          setPhase("unavailable");
        }
      });
      return () => controller.abort();
    }
    invitationPreview(token, controller.signal)
      .then((body) => {
        if (controller.signal.aborted) {
          return;
        }
        setPreview(body);
        setPhase("ready");
      })
      .catch(() => {
        if (!controller.signal.aborted) {
          setPhase("unavailable");
        }
      });
    return () => controller.abort();
  }, [token]);

  const accept = useCallback(async () => {
    setStatus("submitting");
    setActionError(null);
    try {
      await invitationsAccept({ token });
      setPhase("accepted");
      router.replace("/app/");
    } catch (error: unknown) {
      setStatus("idle");
      const forbidden = typeof error === "object" && error !== null && "code" in error && (error as { code?: unknown }).code === "NOT_FOUND";
      setActionError(forbidden ? t("invite.error.invalid") : t("invite.error.generic"));
    }
  }, [token, t, router]);

  if (phase === "loading") {
    return (
      <AuthShell title={t("invite.title")}>
        <div aria-hidden="true" className="flex flex-col gap-3">
          <div className="bg-background-secondary-default h-8 w-48 rounded-md" />
          <div className="bg-background-secondary-default h-4 w-64 rounded-md" />
          <div className="bg-background-secondary-default h-10 w-40 rounded-md" />
        </div>
      </AuthShell>
    );
  }

  if (phase === "unavailable") {
    return (
      <AuthShell title={t("invite.title")}>
        <RiErrorWarningLine className="text-foreground-icon-primary h-8 w-8" aria-hidden="true" />
        <p className="text-text-primary text-body-regular">{t("invite.unavailable")}</p>
        <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary" href="/sign-in">
          {t("signin.submit")}
        </Link>
      </AuthShell>
    );
  }

  const busy = status === "submitting";
  return (
    <AuthShell title={t("invite.title")} subtitle={t("invite.subtitle")}>
      <div className="flex flex-col gap-2">
        <p className="text-text-secondary text-body-2-medium text-caption-1-semibold uppercase tracking-wide">{t("invite.space")}</p>
        <p className="text-text-primary text-headline-medium break-words">{preview?.spaceName}</p>
        {preview?.inviterName ? (
          <p className="text-text-secondary text-body-regular">{t("invite.inviter")} {preview.inviterName}</p>
        ) : null}
        <p className="text-text-tertiary text-body-2-medium">
          {t("invite.role")} {preview?.role} · {t("invite.expires")} {preview?.expiresAt}
        </p>
      </div>
      {actionError ? <p className="text-text-error-primary text-body-2-medium" role="alert">{actionError}</p> : null}
      <Button variant="primary" size="medium" onClick={accept} disabled={busy}>
        {busy ? t("invite.accepting") : t("invite.accept")}
      </Button>
      <div className="flex flex-col gap-1 text-body-2-medium">
        <span className="text-text-tertiary">{t("invite.need-account")}</span>
        <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary" href={`/sign-in?next=${encodeURIComponent(`/invite?token=${token}`)}`}>
          {t("signin.submit")}
        </Link>
        <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary" href={`/sign-up?next=${encodeURIComponent(`/invite?token=${token}`)}`}>
          {t("signup.submit")}
        </Link>
      </div>
    </AuthShell>
  );
}
