"use client";

import Link from "next/link";
import { RiErrorWarningLine, RiRefreshLine, RiCheckLine, RiCloseLine } from "@remixicon/react";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import {
  adminPluginsDecide,
  adminPluginsList,
  type AdminPluginSubmissionView,
} from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

type Phase = "loading" | "ready" | "error" | "forbidden";
type Decision = "approved" | "rejected";

/**
 * `/admin/plugins` (spec/08-ui-ux.md §8.7, REQ-058).
 *
 * The submissions queue, newest first. A decision never executes anything: approving flips the
 * review status so the ordinary install flow can pick the plugin up, rejecting closes it.
 * The source is shown exactly as submitted — it is text to read, never something to render.
 */
export function PluginsScreen() {
  const t = useTranslation();

  const [submissions, setSubmissions] = useState<AdminPluginSubmissionView[]>([]);
  const [phase, setPhase] = useState<Phase>("loading");
  const [reloadToken, setReloadToken] = useState(0);
  const [deciding, setDeciding] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const reload = useCallback(() => {
    setPhase("loading");
    setReloadToken((token) => token + 1);
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    adminPluginsList({ status: "pending" }, controller.signal)
      .then((reply) => {
        if (controller.signal.aborted) {
          return;
        }
        setSubmissions(reply.submissions);
        setPhase("ready");
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) {
          return;
        }
        const forbidden = typeof error === "object" && error !== null && "code" in error && (error as { code?: unknown }).code === "FORBIDDEN";
        setPhase(forbidden ? "forbidden" : "error");
      });

    return () => {
      controller.abort();
    };
  }, [reloadToken]);

  const decide = useCallback(
    async (submissionId: string, decision: Decision) => {
      setDeciding(submissionId);
      setActionError(null);
      try {
        await adminPluginsDecide({ submissionId, decision });
        setSubmissions((current) => current.filter((submission) => submission.id !== submissionId));
      } catch {
        setActionError(t("admin.plugins.error"));
      } finally {
        setDeciding(null);
      }
    },
    [t],
  );

  return (
    <div className="bg-background-full min-h-dvh">
      <header className="border-separator-border border-b">
        <nav
          aria-label={t("shell.nav.primary")}
          className="mx-auto flex w-full max-w-5xl items-center justify-between gap-2 px-4 py-3"
        >
          <div className="flex items-center gap-4">
            <Link href="/" className="boma-target boma-focus text-text-primary hover:text-text-secondary text-headline-medium inline-flex items-center rounded-md px-2 py-1">
              {t("app.name")}
            </Link>
            <span className="text-text-secondary text-body-2-medium uppercase tracking-wide">{t("admin.plugins.title")}</span>
          </div>
          <Link href="/admin/access" className="boma-target boma-focus text-text-primary hover:bg-background-secondary-hover inline-flex items-center rounded-md px-3 py-1.5 text-body-medium">
            {t("admin.access.title")}
          </Link>
        </nav>
      </header>

      <main className="mx-auto flex w-full max-w-5xl flex-col gap-6 px-4 py-6">
        <h1 className="text-title-3-semibold text-text-primary">{t("admin.plugins.title")}</h1>

        {phase === "loading" ? (
          <div aria-hidden="true" className="flex flex-col gap-4">
            <div className="bg-background-secondary-default h-24 w-full rounded-3xl" />
            <div className="bg-background-secondary-default h-24 w-full rounded-3xl" />
          </div>
        ) : null}

        {phase === "forbidden" ? (
          <div className="border-border-error-default flex flex-col items-start gap-2 rounded-xl border p-4">
            <RiErrorWarningLine className="text-foreground-icon-primary h-6 w-6" aria-hidden="true" />
            <p className="text-text-primary text-body-medium">{t("admin.plugins.forbidden")}</p>
          </div>
        ) : null}

        {phase === "error" ? (
          <div className="border-border-error-default flex flex-col items-start gap-2 rounded-xl border p-4">
            <RiErrorWarningLine className="text-foreground-icon-primary h-6 w-6" aria-hidden="true" />
            <p className="text-text-primary text-body-medium">{t("admin.plugins.error")}</p>
            <Button variant="secondary" size="medium" leadingIcon={RiRefreshLine} onClick={reload}>
              {t("common.retry")}
            </Button>
          </div>
        ) : null}

        {phase === "ready" ? (
          submissions.length === 0 ? (
            <p className="text-text-tertiary text-body-regular">{t("admin.plugins.empty")}</p>
          ) : (
            <ol className="divide-separator-border divide-y rounded-3xl border border-border-button-default">
              {submissions.map((submission) => (
                <li key={submission.id} className="flex flex-col gap-3 px-4 py-4 sm:flex-row sm:items-center sm:justify-between">
                  <div className="flex min-w-0 flex-col gap-1">
                    <span className="text-text-primary text-body-medium">{submission.name ?? submission.source}</span>
                    <code dir="auto" className="text-text-secondary text-body-2-regular break-all">{submission.source}</code>
                    <span className="text-text-tertiary text-caption-1-semibold uppercase tracking-wide">{submission.status}</span>
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    <Button
                      variant="primary"
                      size="small"
                      leadingIcon={RiCheckLine}
                      onClick={() => decide(submission.id, "approved")}
                      disabled={deciding === submission.id}
                    >
                      {t("admin.plugins.approve")}
                    </Button>
                    <Button
                      variant="danger"
                      size="small"
                      leadingIcon={RiCloseLine}
                      onClick={() => decide(submission.id, "rejected")}
                      disabled={deciding === submission.id}
                    >
                      {t("admin.plugins.reject")}
                    </Button>
                  </div>
                </li>
              ))}
            </ol>
          )
        ) : null}

        {actionError ? (
          <p className="text-text-error-primary text-body-2-medium" role="alert">{actionError}</p>
        ) : null}
      </main>
    </div>
  );
}
