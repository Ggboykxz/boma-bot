"use client";

import { RiComputerLine, RiErrorWarningLine, RiPlayCircleLine, RiShieldKeyholeLine } from "@remixicon/react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import {
  computerOpenScreen,
  computerOpenScreenFrame,
  computerStatus,
  computerTakeover,
  computerRelease,
  type ComputerStatusReply,
} from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

/** How often the screen refreshes, when the computer is on (spec §8.3; reduced motion is honoured). */
const FRAME_POLL_MS = 2_500;
const STATUS_POLL_MS = 5_000;

const LIVE_STATUSES = new Set(["queued", "running", "interrupted"]);

/**
 * The computer panel of one bot (spec §8.3, §8.4; REQ-026, REQ-030, REQ-029).
 *
 * The panel is only rendered when the deployment has a computer backend and this bot has a
 * computer: no backend at all answers `503` and the panel is absent; `stopped` means the bot
 * has one that is off today and the panel stays out of the way until it starts.
 */
export function ComputerPanel({ botId }: { botId: string }) {
  const t = useTranslation();
  const [status, setStatus] = useState<ComputerStatusReply | null>(null);
  const [frame, setFrame] = useState<string | null>(null);
  const [presence, setPresence] = useState<"unknown" | "supported" | "absent">("unknown");
  const [actionError, setActionError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const frameCapability = useRef<string | null>(null);

  const loadStatus = useCallback(async () => {
    try {
      const body = await computerStatus({ botId });
      setStatus(body);
      setPresence("supported");
      return body.status;
    } catch (error: unknown) {
      const code = typeof error === "object" && error !== null && "code" in error ? (error as { code?: unknown }).code : null;
      if (code === "COMPUTERS_UNAVAILABLE") {
        setPresence("absent");
        return "absent";
      }
      setPresence("absent");
      return "absent";
    }
  }, [botId]);

  const loadFrame = useCallback(async () => {
    if (!frameCapability.current) {
      try {
        const opened = await computerOpenScreen({ botId });
        frameCapability.current = opened.capability;
      } catch {
        return;
      }
    }
    const frameData = await computerOpenScreenFrame(frameCapability.current);
    if (frameData) {
      setFrame(frameData);
    } else if (frameCapability.current) {
      // A stale capability (the generation bumped or the hold works elsewhere) is not fatal:
      // drop it and re-mint on the next round.
      frameCapability.current = null;
    }
  }, [botId]);

  useEffect(() => {
    let statusTimer: ReturnType<typeof setInterval> | null = null;
    let frameTimer: ReturnType<typeof setInterval> | null = null;
    let cancelled = false;
    void Promise.resolve().then(() => loadStatus()).then((current) => {
      if (cancelled) {
        return;
      }
      statusTimer = setInterval(() => {
        void loadStatus();
      }, STATUS_POLL_MS);
      if (current && LIVE_STATUSES.has(current)) {
        void loadFrame();
        frameTimer = setInterval(() => {
          void loadFrame();
        }, FRAME_POLL_MS);
      }
    });
    return () => {
      cancelled = true;
      if (statusTimer) {
        clearInterval(statusTimer);
      }
      if (frameTimer) {
        clearInterval(frameTimer);
      }
    };
  }, [botId, loadStatus, loadFrame]);

  const takeOver = useCallback(async () => {
    setBusy(true);
    setActionError(null);
    try {
      const reply = await computerTakeover({ botId });
      setStatus((current) =>
        current ? { ...current, takeoverByUser: reply.takeoverByUser ?? null, takeoverAt: reply.takeoverAt ?? null } : current,
      );
    } catch {
      setActionError(t("computer.error.takeover"));
    } finally {
      setBusy(false);
    }
  }, [botId, t]);

  const release = useCallback(async () => {
    setBusy(true);
    setActionError(null);
    try {
      const reply = await computerRelease({ botId });
      setStatus((current) =>
        current ? { ...current, takeoverByUser: reply.takeoverByUser ?? null, takeoverAt: reply.takeoverAt ?? null } : current,
      );
    } catch {
      setActionError(t("computer.error.release"));
    } finally {
      setBusy(false);
    }
  }, [botId, t]);

  if (presence === "absent" || status === null || !LIVE_STATUSES.has(status.status)) {
    return null;
  }

  const heldBy = status.takeoverByUser ?? null;
  const isHeld = heldBy !== null;

  return (
    <section aria-label={t("computer.panel.label")} className="border-border-button-default bg-background-primary-default flex flex-col gap-2 rounded-3xl border p-3">
      <div className="flex flex-wrap items-center gap-2">
        <RiComputerLine className="text-foreground-icon-secondary h-4 w-4" aria-hidden="true" />
        <span className="text-text-primary text-body-2-semibold">{t("computer.panel.label")}</span>
        <span role="status" className="text-text-secondary text-body-2-medium inline-flex items-center gap-1">
          <RiPlayCircleLine className="h-4 w-4" aria-hidden="true" />
          {t(`computer.status.${status.status}`)}
        </span>
        <span className="ms-auto" />
        {isHeld ? (
          <span className="text-text-secondary text-body-2-medium inline-flex items-center gap-1">
            <RiShieldKeyholeLine className="h-4 w-4" aria-hidden="true" />
            {t("computer.held-by")} {heldBy}
          </span>
        ) : null}
      </div>

      {frame ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={frame}
          alt={t("computer.panel.frame-alt")}
          className="max-h-64 max-w-full self-center rounded-2xl border border-border-button-default object-contain"
        />
      ) : (
        <div className="bg-background-secondary-default flex h-40 items-center justify-center rounded-2xl">
          <span className="text-text-tertiary text-body-2-medium">{t("computer.panel.no-frame")}</span>
        </div>
      )}

      {actionError ? (
        <p role="alert" className="text-text-error-primary text-body-2-medium flex items-center gap-1">
          <RiErrorWarningLine className="h-4 w-4" aria-hidden="true" />
          {actionError}
        </p>
      ) : null}

      <div className="flex flex-wrap items-center gap-2">
        <Button variant="secondary" onClick={() => void takeOver()} disabled={busy || isHeld}>
          {t("computer.takeover")}
        </Button>
        {isHeld ? (
          <Button variant="ghost" onClick={() => void release()} disabled={busy}>
            {t("computer.release")}
          </Button>
        ) : null}
        {isHeld ? <p className="text-text-tertiary text-body-2-medium">{t("computer.takeover-held")}</p> : null}
      </div>
    </section>
  );
}
