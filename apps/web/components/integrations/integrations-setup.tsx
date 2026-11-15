"use client";

import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { RiCheckLine, RiExternalLinkLine } from "@remixicon/react";
import { useEffect, useState } from "react";

import { AuthShell } from "@/components/auth/auth-shell";
import {
  connectionsList,
  marketplaceList,
  messagingList,
  type ConnectorView,
  type MarketplaceListReply,
  type MessagingInstallation,
} from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

type Phase = "loading" | "ready" | "partial";

/** Which messaging installs landed, if any — what the OAuth callback told us. */
function installedFromParam(installed: string | null): Set<string> {
  if (!installed) {
    return new Set();
  }
  return new Set(
    installed
      .split(",")
      .map((part) => part.trim().toLowerCase())
      .filter(Boolean),
  );
}

/**
 * `/integrations/setup` (spec §8.7, REQ-032, REQ-034, REQ-048).
 *
 * The post-install landing: what is installed, what was skipped, and — when the OAuth callback
 * named a platform — a confirmation with the workspace name the platform reports. Installed
 * plugins' skipped parts are the partial-install report, not a failure; nothing here is a
 * false negative.
 *
 * Per-bot assignment: messaging installs bind one bot at install time; connectors and plugins
 * are space-wide — every bot of the space holds their tools. The screen says both plainly
 * instead of pretending there is a per-bot gate that does not exist.
 */
export function IntegrationsSetupScreen() {
  const t = useTranslation();
  const router = useRouter();
  const searchParams = useSearchParams();
  const justInstalled = installedFromParam(searchParams.get("installed"));

  const [phase, setPhase] = useState<Phase>("loading");
  const [connections, setConnections] = useState<ConnectorView[]>([]);
  const [marketplace, setMarketplace] = useState<MarketplaceListReply | null>(null);
  const [installations, setInstallations] = useState<MessagingInstallation[]>([]);

  useEffect(() => {
    const controller = new AbortController();
    Promise.allSettled([
      connectionsList(controller.signal),
      marketplaceList(controller.signal),
      messagingList(controller.signal),
    ]).then(([connectionsResult, marketplaceResult, messagingResult]) => {
      if (controller.signal.aborted) {
        return;
      }
      if (connectionsResult.status === "fulfilled") {
        setConnections(connectionsResult.value.connections);
      }
      if (marketplaceResult.status === "fulfilled") {
        setMarketplace(marketplaceResult.value);
        if (marketplaceResult.value.installedPlugins.some((plugin) => plugin.skipped.length > 0)) {
          setPhase("partial");
        }
      }
      if (messagingResult.status === "fulfilled") {
        setInstallations(messagingResult.value.installations);
      }
      setPhase((current) => (current === "partial" ? "partial" : "ready"));
    });
    return () => controller.abort();
  }, []);

  if (phase === "loading") {
    return (
      <main className="mx-auto flex w-full max-w-2xl flex-col gap-4 p-4 sm:p-6" aria-busy="true">
        <div className="bg-background-secondary-default h-8 w-64 max-w-full rounded-md" />
        <div className="bg-background-secondary-default h-24 w-full rounded-2xl" />
        <div className="bg-background-secondary-default h-24 w-full rounded-2xl" />
      </main>
    );
  }

  const activeInstallations = installations.filter((installation) => installation.status === "active");
  const installedPlugins = marketplace?.installedPlugins ?? [];

  return (
    <main className="mx-auto flex w-full max-w-2xl flex-col gap-6 p-4 sm:p-6">
      <header className="flex flex-col gap-1">
        <Link href="/app/" className="boma-target boma-focus text-text-tertiary hover:text-text-secondary text-body-2-medium inline-flex items-center gap-1 rounded-md w-fit">
          {t("common.back-to-app")}
        </Link>
        <h1 className="text-title-3-semibold text-text-primary">{t("integrations.title")}</h1>
        <p className="text-text-secondary text-body-regular">{t("integrations.subtitle")}</p>
      </header>

      {justInstalled.size > 0 ? (
        <section className="border-separator-border flex flex-col gap-1 rounded-2xl border px-4 py-3" aria-live="polite">
          <p className="text-text-primary text-body-medium flex items-center gap-2">
            <RiCheckLine className="text-foreground-icon-secondary h-4 w-4" aria-hidden="true" />
            {t("integrations.installed-heading")} {[...justInstalled].join(", ")}
          </p>
          <p className="text-text-secondary text-body-2-medium">{t("integrations.installed-body")}</p>
        </section>
      ) : null}

      {activeInstallations.length > 0 ? (
        <section className="flex flex-col gap-2">
          <h2 className="text-headline-medium text-text-primary">{t("integrations.messaging")}</h2>
          <ul className="flex flex-col gap-2">
            {activeInstallations.map((installation) => (
              <li
                key={installation.id}
                className="border-border-button-default bg-background-primary-default flex items-center justify-between gap-3 rounded-2xl border px-4 py-3"
              >
                <div className="flex min-w-0 flex-col gap-0.5">
                  <span className="text-text-primary text-body-medium break-words">
                    {installation.platform} · {installation.workspaceName || installation.workspaceId}
                  </span>
                  <span className="text-text-tertiary text-body-2-medium">
                    {t("integrations.bound-to")} <code className="text-text-secondary">{installation.botId}</code>
                  </span>
                </div>
              </li>
            ))}
          </ul>
          <p className="text-text-tertiary text-body-2-medium">{t("integrations.messaging-note")}</p>
        </section>
      ) : null}

      {installedPlugins.length > 0 ? (
        <section className="flex flex-col gap-2">
          <h2 className="text-headline-medium text-text-primary">{t("integrations.plugins")}</h2>
          <ul className="flex flex-col gap-2">
            {installedPlugins.map((plugin) => (
              <li
                key={plugin.name}
                className="border-border-button-default bg-background-primary-default flex flex-col gap-2 rounded-2xl border px-4 py-3"
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="text-text-primary text-body-medium">{plugin.name}</span>
                  <a
                    className="boma-target boma-focus text-text-secondary hover:text-text-primary inline-flex items-center gap-1 text-body-2-medium rounded-md"
                    href={plugin.source}
                    target="_blank"
                    rel="noopener noreferrer nofollow"
                  >
                    {t("integrations.source")}
                    <RiExternalLinkLine className="h-4 w-4" aria-hidden="true" />
                  </a>
                </div>
                <p className="text-text-secondary text-body-2-medium">
                  {t("integrations.skills")} {plugin.skillCount} · {t("integrations.mcp")} {plugin.mcpServerCount}
                </p>
                {plugin.skipped.length > 0 ? (
                  <ul className="flex flex-col gap-1">
                    {plugin.skipped.map((part, index) => (
                      <li key={index} className="text-text-tertiary text-body-2-medium">
                        {t("integrations.skipped")} {String(part["part"] ?? part)}
                        {typeof part["reason"] === "string" ? ` — ${part["reason"]}` : ""}
                      </li>
                    ))}
                  </ul>
                ) : null}
              </li>
            ))}
          </ul>
          <p className="text-text-tertiary text-body-2-medium">{t("integrations.plugins-note")}</p>
        </section>
      ) : null}

      {connections.length > 0 ? (
        <section className="flex flex-col gap-2">
          <h2 className="text-headline-medium text-text-primary">{t("integrations.connectors")}</h2>
          <ul className="flex flex-col gap-2">
            {connections.map((connector) => (
              <li
                key={connector.connectorId}
                className="border-border-button-default bg-background-primary-default flex items-center justify-between gap-3 rounded-2xl border px-4 py-3"
              >
                <div className="flex flex-col gap-0.5">
                  <span className="text-text-primary text-body-medium">{connector.name}</span>
                  <span className="text-text-tertiary text-body-2-medium">{connector.description}</span>
                </div>
                <span
                  className={
                    connector.installed
                      ? "text-text-primary text-body-2-medium"
                      : "text-text-tertiary text-body-2-medium"
                  }
                >
                  {connector.installed ? t("integrations.installed-tag") : t("integrations.not-installed-tag")}
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </main>
  );
}
