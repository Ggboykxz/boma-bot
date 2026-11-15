"use client";

import { RiAddLine, RiRobotLine } from "@remixicon/react";
import { useEffect, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import {
  botsConnectExternal,
  botsDisconnectExternal,
  botsExternalAgentStatus,
  type ExternalAgentStatus,
} from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

/**
 * The external-agent panel for one bot (M12.2, REQ-059).
 *
 * The RPC carries the connection state and the **bearer's secret name**; the bearer itself
 * lives inside the space's bot-secret vault. This component never renders, stores or logs a
 * secret value; the only fields it touches are URLs and names.
 */
export function ExternalAgentPanel({ botId }: { botId: string }) {
  const t = useTranslation();
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<ExternalAgentStatus | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [agentName, setAgentName] = useState("");
  const [secretName, setSecretName] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let active = true;
    botsExternalAgentStatus({ botId })
      .then((value) => {
        if (active) {
          setStatus(value);
        }
      })
      .catch(() => {
        if (active) {
          setStatus({ connected: false });
        }
      });
    return () => {
      active = false;
    };
  }, [botId]);

  const attach = async () => {
    setBusy(true);
    setFailed(false);
    try {
      await botsConnectExternal({
        botId,
        baseUrl: baseUrl.trim(),
        agentName: agentName.trim() || undefined,
        secretName: secretName.trim() || undefined,
      });
      const next = await botsExternalAgentStatus({ botId });
      setStatus(next);
      setBaseUrl("");
      setAgentName("");
      setSecretName("");
      setOpen(false);
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  const detach = async () => {
    setBusy(true);
    setFailed(false);
    try {
      await botsDisconnectExternal({ botId });
      setStatus((await botsExternalAgentStatus({ botId })) ?? { connected: false });
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  if (status === null) {
    return (
      <div
        className="bg-background-secondary-default h-8 w-36 rounded-full"
        aria-label={t("thread.external-agent.label")}
        aria-busy="true"
      />
    );
  }

  if (status.connected) {
    return (
      <div className="border-border-separator-border border-separator-border flex flex-wrap items-center gap-2 rounded-full border px-3 py-1 text-caption-1-semibold text-text-secondary">
        <RiRobotLine size={16} aria-hidden className="text-foreground-icon-secondary" />
        <span className="text-text-primary truncate">{status.agentName ?? status.baseUrl}</span>
        <Button
          type="button"
          variant="ghost"
          size="xs"
          onClick={() => void detach()}
          disabled={busy}
          aria-label={t("thread.external-agent.disconnect")}
        >
          {t("thread.external-agent.disconnect")}
        </Button>
        {failed ? <span role="alert" className="text-text-error-primary">{t("thread.error.send")}</span> : null}
      </div>
    );
  }

  return (
    <div className="relative">
      <Button
        type="button"
        variant="secondary"
        size="xs"
        iconOnly={false}
        leadingIcon={RiAddLine}
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
      >
        {t("thread.external-agent.attach")}
      </Button>
      {open ? (
        <div
          role="dialog"
          aria-label={t("thread.external-agent.attach")}
          className="border-border-button-default absolute z-30 mt-8 w-80 rounded-xl border bg-background-primary-default p-3 shadow-lg"
        >
          <div className="flex flex-col gap-2">
            <label htmlFor="ea-baseurl" className="text-xs font-semibold text-text-primary">
              {t("thread.external-agent.base-url")}
              <span className="text-text-tertiary"> {t("thread.external-agent.required")}</span>
            </label>
            <input
              id="ea-baseurl"
              value={baseUrl}
              onChange={(event) => setBaseUrl(event.target.value)}
              className="placeholder:text-text-placeholder focus:border-border-focus-ring rounded-md border border-border-button-default bg-background-secondary-default px-2 py-2 text-body-2-regular outline-none"
              placeholder="https://agents.internal.example.com/agui"
              autoFocus
            />
            <label htmlFor="ea-agentname" className="text-xs font-semibold text-text-primary">
              {t("thread.external-agent.agent-name")}
            </label>
            <input
              id="ea-agentname"
              value={agentName}
              onChange={(event) => setAgentName(event.target.value)}
              className="placeholder:text-text-placeholder focus:border-border-focus-ring rounded-md border border-border-button-default bg-background-secondary-default px-2 py-2 text-body-2-regular outline-none"
              placeholder="release-bot"
            />
            <label htmlFor="ea-secretname" className="text-xs font-semibold text-text-primary">
              {t("thread.external-agent.secret-name")}
            </label>
            <input
              id="ea-secretname"
              value={secretName}
              onChange={(event) => setSecretName(event.target.value)}
              className="placeholder:text-text-placeholder focus:border-border-focus-ring rounded-md border border-border-button-default bg-background-secondary-default px-2 py-2 text-body-2-regular outline-none"
              placeholder="BOT_EXECUTOR_KEY"
            />
            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="primary"
                size="small"
                disabled={busy || baseUrl.trim().length === 0}
                onClick={() => void attach()}
              >
                {busy ? t("thread.external-agent.attaching") : t("thread.external-agent.attach")}
              </Button>
              <Button type="button" variant="ghost" size="small" onClick={() => { setOpen(false); setFailed(false); }}>
                {t("common.retry")}
              </Button>
            </div>
            {failed ? (
              <p role="alert" className="text-text-error-primary text-xs">{t("thread.error.send")}</p>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}
