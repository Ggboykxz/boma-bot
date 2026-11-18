"use client";

import { useEffect, useState, useSyncExternalStore } from "react";

import { Button } from "@/components/base/buttons/button";
import { botsConnectAcp, botsDetectedExecutors } from "@/lib/api/rpc";
import { listRoster, refreshRoster, serverRoster, subscribeRoster } from "@/lib/roster";
import { useTranslation } from "@/lib/use-translation";

/**
 * Detected local ACP executors (spec/01 REQ-061).
 *
 * Detection is server-side and best-effort; this row renders nothing when none are found. Nothing
 * is attached without an explicit click, and a remote deployment simply has no executor here.
 */
export function AcpDetectedRow() {
  const t = useTranslation();
  const [executors, setExecutors] = useState<string[]>([]);
  const [attached, setAttached] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const roster = useSyncExternalStore(subscribeRoster, listRoster, serverRoster);

  useEffect(() => {
    const controller = new AbortController();
    botsDetectedExecutors(controller.signal)
      .then((reply) => setExecutors(reply.executors))
      .catch(() => setExecutors([]));
    return () => controller.abort();
  }, []);

  if (executors.length === 0) {
    return null;
  }

  const target = roster?.find((bot) => !bot.archivedAt && !bot.snoozedUntil);

  const attach = async (executor: string) => {
    if (!target || working) {
      return;
    }
    setWorking(true);
    try {
      await botsConnectAcp({ botId: target.id, executor });
      await refreshRoster();
      setAttached(executor);
    } catch {
      // The row stays; the operator can retry.
    } finally {
      setWorking(false);
    }
  };

  return (
    <div className="flex flex-col gap-1">
      {executors.map((executor) => (
        <div key={executor} className="flex items-center justify-between gap-1">
          <span className="text-text-secondary text-caption-1-regular truncate">
            {executor} — {t("shell.rail.acp.detected")}
          </span>
          <Button
            type="button"
            size="small"
            variant="secondary"
            disabled={!target || working || attached === executor}
            onClick={() => void attach(executor)}
          >
            {attached === executor ? t("shell.rail.acp.attached") : t("shell.rail.acp.attach")}
          </Button>
        </div>
      ))}
      {executors.length > 0 && !target ? (
        <p className="text-text-tertiary text-body-2-regular">{t("shell.rail.acp.no-bot")}</p>
      ) : null}
    </div>
  );
}
