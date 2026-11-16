"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import {
  RiFileLine,
  RiFilePdf2Line,
  RiFileTextLine,
  RiImageLine,
  RiLayoutGridLine,
  RiSearchLine,
} from "@remixicon/react";

import { AuthShell } from "@/components/auth/auth-shell";
import {
  artifactsListSpace,
  botsList,
  type ArtifactView,
  type RosterBotView,
} from "@/lib/api/rpc";
import { clip, formatBytes, shortDate, typeBucket } from "@/components/artifacts/artifacts-shared";
import { useTranslation } from "@/lib/use-translation";

type TypeFilter = "all" | "image" | "pdf" | "text" | "other";

/**
 * `/app/artifacts` (spec §8.5). The document is a grid of the artifacts the space produced:
 * the bot, the runtime proof and nothing else. The latest version of every family is here;
 * older ones live on the viewer's version selector.
 */
export function ArtifactsGrid() {
  const t = useTranslation();
  const [phase, setPhase] = useState<"loading" | "ready" | "error">("loading");
  const [artifacts, setArtifacts] = useState<ArtifactView[]>([]);
  const [bots, setBots] = useState<RosterBotView[]>([]);
  const [query, setQuery] = useState("");
  const [type, setType] = useState<TypeFilter>("all");
  const [bot, setBot] = useState<string>("all");

  useEffect(() => {
    const controller = new AbortController();
    Promise.allSettled([artifactsListSpace(controller.signal), botsList(controller.signal)]).then(
      ([listResult, botsResult]) => {
        if (controller.signal.aborted) {
          return;
        }
        if (listResult.status === "rejected") {
          setPhase("error");
          return;
        }
        setArtifacts(listResult.value.artifacts);
        if (botsResult.status === "fulfilled") {
          setBots(botsResult.value.bots);
        }
        setPhase("ready");
      },
    );
    return () => controller.abort();
  }, []);

  const botName = (id: string | undefined): string =>
    id ? (bots.find((entry) => entry.id === id)?.name ?? id) : "—";

  const filtered = artifacts.filter((artifact) => {
    if (type !== "all" && typeBucket(artifact.mimeType) !== type) {
      return false;
    }
    if (bot !== "all" && artifact.botId !== bot) {
      return false;
    }
    if (query.trim() && !artifact.name.toLowerCase().includes(query.trim().toLowerCase())) {
      return false;
    }
    return true;
  });

  const usedBots = [...new Set(artifacts.map((artifact) => artifact.botId).filter((id): id is string => typeof id === "string"))];

  if (phase === "loading") {
    return (
      <section className="flex h-full flex-col gap-4 p-4 sm:p-6" aria-busy="true">
        <div className="flex flex-col gap-2">
          <div className="bg-background-secondary-default h-8 w-48 rounded-md" />
          <div className="bg-background-secondary-default h-4 w-96 max-w-full rounded-md" />
        </div>
        <ul className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {[0, 1, 2, 3, 4, 5].map((index) => (
            <li key={index} className="border-border-button-default bg-background-primary-default flex h-40 rounded-2xl border p-4">
              <div className="flex flex-col gap-2">
                <div className="bg-background-secondary-default h-4 w-32 rounded-md" />
                <div className="bg-background-secondary-default h-3 w-48 rounded-md" />
              </div>
            </li>
          ))}
        </ul>
      </section>
    );
  }

  if (phase === "error") {
    return (
      <section className="flex h-full flex-col gap-3 p-4 sm:p-6">
        <p className="text-text-error-primary text-body-medium" role="alert">{t("artifacts.error.load")}</p>
      </section>
    );
  }

  return (
    <section className="flex h-full min-w-0 flex-col gap-4 overflow-y-auto p-4 sm:p-6">
      <header className="flex flex-col gap-1">
        <h1 className="text-title-3-semibold text-text-primary flex items-center gap-2">
          <RiLayoutGridLine className="text-foreground-icon-primary h-5 w-5" aria-hidden="true" />
          {t("artifacts.title")}
        </h1>
        <p className="text-text-secondary text-body-regular">{t("artifacts.subtitle")}</p>
      </header>

      <div className="flex flex-wrap items-center gap-2">
        <label className="border-border-button-default bg-background-primary-default flex h-9 min-w-48 flex-1 items-center gap-2 rounded-md border px-3">
          <RiSearchLine className="text-foreground-icon-tertiary h-4 w-4" aria-hidden="true" />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("artifacts.search")}
            aria-label={t("artifacts.search")}
            className="placeholder:text-text-placeholder w-full border-0 bg-transparent text-body-regular outline-none"
          />
        </label>
        <select
          aria-label={t("artifacts.filter.type")}
          value={type}
          onChange={(event) => setType(event.target.value as TypeFilter)}
          className="boma-focus border-border-button-default bg-background-primary-default h-9 rounded-md border px-2 text-body-regular"
        >
          <option value="all">{t("artifacts.filter.all-types")}</option>
          <option value="image">{t("artifacts.filter.images")}</option>
          <option value="pdf">{t("artifacts.filter.pdfs")}</option>
          <option value="text">{t("artifacts.filter.text")}</option>
          <option value="other">{t("artifacts.filter.other")}</option>
        </select>
        <select
          aria-label={t("artifacts.filter.bot")}
          value={bot}
          onChange={(event) => setBot(event.target.value)}
          className="boma-focus border-border-button-default bg-background-primary-default h-9 rounded-md border px-2 text-body-regular"
        >
          <option value="all">{t("artifacts.filter.all-bots")}</option>
          {usedBots.map((id) => (
            <option key={id} value={id}>{botName(id)}</option>
          ))}
        </select>
      </div>

      {filtered.length === 0 ? (
        <p className="text-text-tertiary text-body-regular">{t("artifacts.empty")}</p>
      ) : (
        <ul className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-3">
          {filtered.map((artifact) => {
            const bucket = typeBucket(artifact.mimeType);
            const icon =
              bucket === "image" ? (
                <RiImageLine className="text-foreground-icon-secondary h-6 w-6" aria-hidden="true" />
              ) : bucket === "pdf" ? (
                <RiFilePdf2Line className="text-foreground-icon-secondary h-6 w-6" aria-hidden="true" />
              ) : bucket === "text" ? (
                <RiFileTextLine className="text-foreground-icon-secondary h-6 w-6" aria-hidden="true" />
              ) : (
                <RiFileLine className="text-foreground-icon-secondary h-6 w-6" aria-hidden="true" />
              );
            return (
              <li key={artifact.id} className="animate-row-grow-in">
                <Link
                  href={`/app/artifacts/${artifact.id}/`}
                  className="boma-focus border-border-button-default bg-background-primary-default hover:bg-background-primary-hover focus-visible:outline-border-button-focus group flex h-full flex-col gap-3 rounded-2xl border p-4 transition-colors focus-visible:outline"
                >
                  <div className="flex items-start justify-between gap-2">
                    {icon}
                    <span className="text-text-tertiary text-body-2-medium">
                      v{artifact.version} · {artifact.id.slice(0, 8)}
                    </span>
                  </div>
                  <div className="flex flex-col gap-1">
                    <span className="text-text-primary text-body-medium line-clamp-2 break-words">{clip(artifact.name, 64)}</span>
                    <span className="text-text-secondary text-body-2-medium">{formatBytes(artifact.size)} · {artifact.mimeType}</span>
                    <span className="text-text-tertiary text-body-2-medium">
                      {botName(artifact.botId)} · <time dateTime={artifact.createdAt}>{shortDate(artifact.createdAt)}</time>
                    </span>
                  </div>
                </Link>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
