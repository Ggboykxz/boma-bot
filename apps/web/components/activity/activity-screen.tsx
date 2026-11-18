"use client";

import { RiErrorWarningLine, RiRefreshLine } from "@remixicon/react";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import { runsList, runsPause, runsResume, runsRetry, type RunView } from "@/lib/api/rpc";
import { isRetryable, runStatusKey, runTriggerKey } from "@/lib/run-status";
import { useTranslation } from "@/lib/use-translation";

type Phase = "loading" | "ready" | "error";

/**
 * The activity screen (spec/08-ui-ux.md §8.4, REQ-011, REQ-012).
 *
 * One list, one row per run, newest first. Every row writes its state out in words: the colour is
 * a second signal, never the only one. The failed row is the only one that offers a retry, because
 * `failed` is the only status a run may leave (REQ-012).
 */
export function ActivityScreen() {
  const t = useTranslation();

  const [runs, setRuns] = useState<RunView[]>([]);
  const [phase, setPhase] = useState<Phase>("loading");
  const [reloadToken, setReloadToken] = useState(0);
  const [retrying, setRetrying] = useState<string | null>(null);
  const [busyRun, setBusyRun] = useState<string | null>(null);

  // The retry button resets the view itself (it is an event handler); the effect only fetches.
  const reload = useCallback(() => {
    setPhase("loading");
    setReloadToken((token) => token + 1);
  }, []);

  useEffect(() => {
    const controller = new AbortController();

    runsList({ limit: 50 }, controller.signal)
      .then((reply) => {
        if (controller.signal.aborted) {
          return;
        }
        setRuns(reply.runs);
        setPhase("ready");
      })
      .catch(() => {
        if (!controller.signal.aborted) {
          setPhase("error");
        }
      });

  return () => {
      controller.abort();
    };
  }, [reloadToken]);

  const retry = useCallback(async (runId: string) => {
    setRetrying(runId);
    try {
      await runsRetry({ runId });
    } catch {
      // The procedure lands with the leases in M2: the row stays, the action reports itself.
    } finally {
      setRetrying(null);
    }
  }, []);

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-headline-medium text-text-primary">{t("shell.nav.activity")}</h1>

      {phase === "loading" ? (
        <div aria-hidden="true" className="flex flex-col gap-3">
          <div className="bg-background-secondary-default h-14 w-full rounded-3xl" />
          <div className="bg-background-secondary-default h-14 w-full rounded-3xl" />
          <div className="bg-background-secondary-default h-14 w-5/6 rounded-3xl" />
        </div>
      ) : null}

      {phase === "error" ? (
        <div className="border-border-error-default flex flex-col items-start gap-2 rounded-xl border p-3">
          <p className="text-text-error-primary text-body-medium flex items-center gap-2">
            <RiErrorWarningLine className="text-foreground-icon-error" size={18} aria-hidden />
            {t("activity.error")}
          </p>
          <Button
            variant="secondary"
            size="small"
            onClick={reload}
          >
            {t("common.retry")}
          </Button>
        </div>
      ) : null}

      {phase === "ready" && runs.length === 0 ? (
        <p className="text-text-secondary text-body-medium">{t("activity.empty")}</p>
      ) : null}

      {runs.length > 0 ? (
        <ul className="flex flex-col gap-2">
          {runs.map((run) => {
            const statusKey = runStatusKey(run.status);
            const triggerKey = runTriggerKey(run.trigger);
            const control = async (id: string, action: "pause" | "resume") => {
    setBusyRun(id);
    try {
      const reply =
        action === "pause" ? await runsPause({ runId: id }) : await runsResume({ runId: id });
      setRuns((current) =>
        current.map((run) => (run.id === reply.id ? { ...run, status: reply.status } : run)),
      );
    } catch {
      // The row keeps its current status; the operator can retry.
    } finally {
      setBusyRun(null);
    }
  };

  return (
              <li
                key={run.id}
                className="animate-row-slide-in border-border-button-default bg-background-primary-default flex flex-col gap-1 rounded-3xl border p-3"
              >
                <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                  <p className="text-text-primary text-body-medium">
                    {run.promptSnippet ??
                      (triggerKey === null ? run.trigger : t(triggerKey))}
                  </p>
                  <p role="status" className="text-text-secondary text-caption-1-semibold">
                    {statusKey === null ? run.status : t(statusKey)}
                  </p>
                </div>
                {run.status === "queued" || run.status === "paused" ? (
                  <div>
                    <Button
                      variant="secondary"
                      size="small"
                      disabled={busyRun === run.id}
                      onClick={() => void control(run.id, run.status === "queued" ? "pause" : "resume")}
                    >
                      {run.status === "queued" ? t("common.pause") : t("common.resume")}
                    </Button>
                  </div>
                ) : null}
                {isRetryable(run.status) ? (
                  <div>
                    <Button
                      variant="secondary"
                      size="small"
                      leadingIcon={RiRefreshLine}
                      disabled={retrying === run.id}
                      onClick={() => void retry(run.id)}
                    >
                      {t("common.retry")}
                    </Button>
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : null}
    </div>
  );
}
