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
  notificationsDestinationsList,
  notificationsDestinationsCreate,
  notificationsDestinationsDelete,
  applicationServerKeyBytes,
  uint8ArrayToBase64Url,
  type NotificationDestinationView,
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
  const [destinations, setDestinations] = useState<NotificationDestinationView[]>([]);
  const [creatingDestination, setCreatingDestination] = useState(false);
  const [destinationEnabledFlags, setDestinationEnabledFlags] = useState<"ready" | "failed">("ready");
  const [newKind, setNewKind] = useState<"slack_webhook" | "teams_webhook">("slack_webhook");
  const [newLabel, setNewLabel] = useState("");
  const [newWebhook, setNewWebhook] = useState("");

  const refreshDestinations = async () => {
    try {
      const reply = await notificationsDestinationsList();
      setDestinations(reply.destinations);
      setDestinationEnabledFlags("ready");
    } catch {
      setDestinations([]);
    }
  };

  const addDestination = async () => {
    const label = newLabel.trim();
    const webhook = newWebhook.trim();
    if (!label || !webhook) {
      return;
    }
    setCreatingDestination(true);
    try {
      await notificationsDestinationsCreate({ kind: newKind, label, webhook });
      setNewLabel("");
      setNewWebhook("");
      await refreshDestinations();
    } catch {
      setDestinationEnabledFlags("failed");
    } finally {
      setCreatingDestination(false);
    }
  };

  const removeDestination = async (id: string) => {
    setCreatingDestination(true);
    try {
      await notificationsDestinationsDelete({ id });
      await refreshDestinations();
    } catch {
      setDestinationEnabledFlags("failed");
    } finally {
      setCreatingDestination(false);
    }
  };

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
        if (active) {
          // Destinations are space-owned; load them independently of the browser state.
          void (async () => {
            try {
              const reply = await notificationsDestinationsList();
              if (active) {
                setDestinations(reply.destinations);
              }
            } catch {
              /* the admins own the webhook; members just see the list */
            }
          })();
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
      await refreshDestinations();
    } catch {
      setPhase("failed");
    }
  };

  const patchPolicy = async (
    patch: Partial<Pick<NotificationsPolicyView, "notifyWaiting" | "notifyCompleted" | "dailyCap" | "destinationSlackWebhook" | "destinationTeamsWebhook">>,
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
            <input
              type="checkbox"
              checked={policy.destinationSlackWebhook}
              onChange={(event) => void patchPolicy({ destinationSlackWebhook: event.target.checked })}
              className="accent-accent-600"
            />
            {t("thread.notifications.destinations.slack")}
          </label>
          <label className="text-text-secondary flex items-center gap-1 text-xs">
            <input
              type="checkbox"
              checked={policy.destinationTeamsWebhook}
              onChange={(event) => void patchPolicy({ destinationTeamsWebhook: event.target.checked })}
              className="accent-accent-600"
            />
            {t("thread.notifications.destinations.teams")}
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
      {phase === "on" ? (
        <div className="border-border-separator-border flex w-full flex-col gap-1 rounded-xl border p-2 text-xs">
          <p className="text-text-secondary text-xs font-semibold" title={t("thread.notifications.destinations.tooltip")}>
            {t("thread.notifications.destinations")}
          </p>
          {destinations.length === 0 ? (
            <p className="text-text-tertiary text-xs">{t("thread.notifications.destinations.empty")}</p>
          ) : null}
          {destinations.map((destination) => (
            <div key={destination.id} className="flex flex-wrap items-center gap-2">
              <span className="rounded-full border border-border-separator-border px-2 py-0.5 text-[10px] uppercase tracking-wide text-text-tertiary">
                {destination.kind.replace("_webhook", "")}
              </span>
              <span className="text-text-primary">{destination.label}</span>
              <button
                type="button"
                onClick={() => void removeDestination(destination.id)}
                disabled={creatingDestination}
                className="boma-focus ml-auto rounded-md px-1 py-0.5 text-text-tertiary transition-colors hover:text-text-error-primary"
                aria-label={`Remove destination ${destination.label}`}
              >
                ×
              </button>
            </div>
          ))}
          <form
            className="mt-1 flex flex-wrap items-center gap-2"
            onSubmit={(event) => {
              event.preventDefault();
              void addDestination();
            }}
          >
            <select
              value={newKind}
              onChange={(event) => setNewKind(event.target.value as typeof newKind)}
              aria-label={t("thread.notifications.destinations.kind")}
              className="rounded-md border border-border-button-default bg-background-secondary-default px-2 py-1 text-xs text-text-primary"
            >
              <option value="slack_webhook">{t("thread.notifications.destinations.slack")}</option>
              <option value="teams_webhook">{t("thread.notifications.destinations.teams")}</option>
            </select>
            <input
              value={newLabel}
              onChange={(event) => setNewLabel(event.target.value)}
              placeholder={t("thread.notifications.destinations.label")}
              aria-label={t("thread.notifications.destinations.label")}
              className="w-28 rounded-md border border-border-button-default bg-background-secondary-default px-2 py-1 text-xs text-text-primary outline-none"
            />
            <input
              value={newWebhook}
              onChange={(event) => setNewWebhook(event.target.value)}
              placeholder="https://"
              aria-label={t("thread.notifications.destinations.url")}
              className="w-40 rounded-md border border-border-button-default bg-background-secondary-default px-2 py-1 text-xs text-text-primary outline-none"
            />
            <Button type="submit" variant="ghost" size="xs" disabled={creatingDestination || !newLabel.trim() || !newWebhook.trim()}>
              {t("thread.notifications.destinations.add")}
            </Button>
          </form>
          {destinationEnabledFlags === "failed" ? (
            <p role="alert" className="text-text-error-primary">{t("thread.error.send")}</p>
          ) : null}
        </div>
      ) : null}
      {phase === "failed" ? (
        <p role="alert" className="text-text-error-primary text-xs">
          {t("thread.error.send")}
        </p>
      ) : null}
    </div>
  );
}
