"use client";

import { RiBellLine, RiBellFill } from "@remixicon/react";
import { useEffect, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import {
  notificationsPolicyGet,
  notificationsPolicySet,
  notificationsStatus,
  notificationsSubscribe,
  notificationsUnsubscribe,
  applicationServerKeyBytes,
  uint8ArrayToBase64Url,
  type NotificationsPolicyView,
} from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

/**
 * The per-bot push notification panel (M15.1, REQ-064).
 *
 * The browser decides what it can do: a `Notification` permission, a service worker registration,
 * and a `pushManager` subscription — the RPC surface exposes the operator's policy gates and the
 * per-(member, bot) caps the server enforces at send time.
 *
 * The component hides behind the deployment's own switch: when the operator runs push disabled,
 * nothing is rendered except a muted note. When the browser cannot push, nothing renders.
 */

function base64UrlBytesToString(bytes: Uint8Array | ArrayBuffer | undefined): string | null {
  if (!bytes) return null;
  const view = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  return uint8ArrayToBase64Url(view);
}

const isPushSupported = (): boolean =>
  typeof window !== "undefined" &&
  "Notification" in window &&
  "serviceWorker" in navigator &&
  window.isSecureContext;

export function NotificationsPanel({ botId }: { botId: string }) {
  const t = useTranslation();
  const [phase, setPhase] = useState<
    "loading" | "unsupported" | "disabled" | "off" | "on" | "busy" | "failed"
  >(() => (isPushSupported() ? "loading" : "unsupported"));
  const [policy, setPolicy] = useState<NotificationsPolicyView | null>(null);

  useEffect(() => {
    let active = true;
    if (!isPushSupported()) {
      return;
    }
    notificationsStatus()
      .then(async (status) => {
        if (!active) {
          return;
        }
        if (!status.enabled) {
          setPhase("disabled");
          return;
        }
        try {
          const registration = await navigator.serviceWorker.getRegistration();
          const existing = await registration?.pushManager?.getSubscription();
          if (active) {
            setPhase(existing ? "on" : "off");
          }
        } catch {
          if (active) {
            setPhase("off");
          }
        }
        const current = await notificationsPolicyGet({ botId }).catch(() => null);
        if (active && current) {
          setPolicy(current);
        }
      })
      .catch(() => {
        if (active) {
          setPhase("disabled");
        }
      });
    return () => {
      active = false;
    };
  }, [botId]);

  const toggle = async () => {
    setPhase("busy");
    try {
      if (phase === "on") {
        const registration = await navigator.serviceWorker.getRegistration();
        const subscription = await registration?.pushManager?.getSubscription();
        if (subscription) {
          await notificationsUnsubscribe({ endpoint: subscription.endpoint });
          await subscription.unsubscribe();
        }
        if (policy) {
          setPhase("off");
        } else {
          setPhase("off");
        }
        return;
      }
      const granted = await Notification.requestPermission();
      if (granted !== "granted") {
        setPhase("off");
        return;
      }
      const status = await notificationsStatus();
      if (!status.enabled || !status.vapidPublicKey) {
        setPhase("disabled");
        return;
      }
      const registration = await navigator.serviceWorker.register("/sw.js");
      const subscription = await registration.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: applicationServerKeyBytes(status.vapidPublicKey) as BufferSource,
      });
      const p256dh = base64UrlBytesToString(subscription.getKey("p256dh") ?? undefined);
      const auth = base64UrlBytesToString(subscription.getKey("auth") ?? undefined);
      if (!p256dh || !auth) {
        setPhase("failed");
        return;
      }
      await notificationsSubscribe({
        endpoint: subscription.endpoint,
        p256dh,
        auth,
        userAgent: navigator.userAgent,
      });
      setPhase("on");
      const current = await notificationsPolicyGet({ botId }).catch(() => null);
      if (current) {
        setPolicy(current);
      }
    } catch {
      setPhase("failed");
    }
  };

  const patchPolicy = async (
    patch: Partial<Pick<NotificationsPolicyView, "notifyWaiting" | "notifyCompleted" | "dailyCap">>,
  ) => {
    const next = await notificationsPolicySet({ botId, ...patch }).catch(() => null);
    if (next) {
      setPolicy(next);
    }
  };

  if (phase === "loading" || phase === "unsupported") {
    return null;
  }

  if (phase === "disabled") {
    return (
      <p className="text-text-secondary text-xs" role="status">
        {t("thread.notifications.unavailable")}
      </p>
    );
  }

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button
        type="button"
        variant={phase === "on" ? "secondary" : "ghost"}
        size="xs"
        leadingIcon={phase === "on" ? RiBellFill : RiBellLine}
        onClick={() => void toggle()}
        disabled={phase === "busy"}
        aria-pressed={phase === "on"}
      >
        {phase === "busy"
          ? t("thread.notifications.enabling")
          : phase === "on"
            ? t("thread.notifications.enabled")
            : t("thread.notifications.enable")}
      </Button>
      {phase === "on" && policy ? (
        <form
          className="border-border-separator-border flex flex-wrap items-center gap-2 rounded-full border px-3 py-1"
          onSubmit={(event) => event.preventDefault()}
        >
          <label className="text-text-secondary flex items-center gap-1 text-xs">
            <input
              type="checkbox"
              checked={policy.notifyWaiting}
              onChange={(event) => void patchPolicy({ notifyWaiting: event.target.checked })}
              className="accent-accent-600"
            />
            {t("thread.notifications.waiting")}
          </label>
          <label className="text-text-secondary flex items-center gap-1 text-xs">
            <input
              type="checkbox"
              checked={policy.notifyCompleted}
              onChange={(event) => void patchPolicy({ notifyCompleted: event.target.checked })}
              className="accent-accent-600"
            />
            {t("thread.notifications.completed")}
          </label>
          <label className="text-text-secondary flex items-center gap-1 text-xs">
            {t("thread.notifications.dailyCap")}
            <input
              type="number"
              min={0}
              max={10000}
              inputMode="numeric"
              value={policy.dailyCap}
              onChange={(event) => void patchPolicy({ dailyCap: Number(event.target.value) })}
              className="w-14 rounded-md border border-border-button-default bg-background-secondary-default px-1 py-0.5 text-xs text-text-primary outline-none"
            />
          </label>
        </form>
      ) : null}
      {phase === "failed" ? (
        <p role="alert" className="text-text-error-primary text-xs">
          {t("thread.error.send")}
        </p>
      ) : null}
    </div>
  );
}
