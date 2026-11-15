"use client";

import { useRouter } from "next/navigation";
import { RiCheckLine, RiSparkling2Line } from "@remixicon/react";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import {
  botsCreate,
  modelsCatalog,
  modelsConnect,
  modelsCredentials,
  setSpaceId,
  spacesCreate,
  type ModelCredentialsReply,
  type ModelsCatalogReply,
} from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

type Step = 1 | 2 | 3;
const STEPS: readonly Step[] = [1, 2, 3];
const STORAGE_KEY = "boma-onboarding";

interface SavedProgress {
  version: 1;
  spaceId?: string;
  botId?: string;
}

/** Reads (and gently self-heals) the wizard's localStorage bookmark. */
function loadProgress(): SavedProgress {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return { version: 1 };
    }
    const parsed = JSON.parse(raw) as SavedProgress | null;
    return parsed && parsed.version === 1 ? parsed : { version: 1 };
  } catch {
    return { version: 1 };
  }
}

function saveProgress(progress: SavedProgress): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(progress));
  } catch {
    // A missing storage slot must not brick onboarding; the wizard still works in memory.
  }
}

/**
 * `/onboarding/<step>` (spec/08-ui-ux.md §8.2, REQ-042, REQ-052).
 *
 * Three independent steps: (1) name the space, (2) create the first bot, (3) connect a model.
 * Steps 1 and 2 are skippable; step 3 is the one that is not — leaving a space without a model
 * would make the bot a mailbox with no mind. Reloading resumes on the same step.
 */
export function OnboardingScreen({ step }: { step: Step }) {
  const t = useTranslation();
  const router = useRouter();

  const [saved, setSaved] = useState<SavedProgress>({ version: 1 });
  const [spaceName, setSpaceName] = useState("");
  const [botName, setBotName] = useState("");
  const [botPurpose, setBotPurpose] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<ModelsCatalogReply | null>(null);
  const [credentials, setCredentials] = useState<ModelCredentialsReply | null>(null);
  const [provider, setProvider] = useState("");
  const [apiKey, setApiKey] = useState("");

  useEffect(() => {
    const loaded = loadProgress();
    // The space choice must reach the client before onboarding step 3 talks to the server.
    if (loaded.spaceId) {
      setSpaceId(loaded.spaceId);
    }
    Promise.resolve().then(() => {
      setSaved(loaded);
    });
    if (step === 3) {
      const controller = new AbortController();
      Promise.allSettled([modelsCatalog(controller.signal), modelsCredentials(controller.signal)]).then(
        ([catalogResult, credentialsResult]) => {
          if (controller.signal.aborted) {
            return;
          }
          if (catalogResult.status === "fulfilled") {
            setCatalog(catalogResult.value);
            setProvider(catalogResult.value.providers[0]?.id ?? "");
          }
          if (credentialsResult.status === "fulfilled") {
            setCredentials(credentialsResult.value);
          }
        },
      );
      return () => controller.abort();
    }
    return () => {};
  }, [step]);

  const goTo = useCallback(
    (next: Step) => {
      router.push(`/onboarding/${next}/`);
    },
    [router],
  );

  const finish = useCallback(() => {
    try {
      window.localStorage.removeItem(STORAGE_KEY);
    } catch {
      // storage gone is fine; the wizard's artifacts live server-side.
    }
    router.push(saved.botId ? `/app/${saved.botId}/` : "/app/");
  }, [router, saved.botId]);

  const createSpace = useCallback(async () => {
    const name = spaceName.trim();
    if (!name) {
      setError(t("onboarding.error.space-name"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const created = await spacesCreate({ name });
      setSpaceId(created.id);
      const progress = { ...saved, version: 1 as const, spaceId: created.id };
      saveProgress(progress);
      setSaved(progress);
      goTo(2);
    } catch {
      setError(t("onboarding.error.space-create"));
    } finally {
      setBusy(false);
    }
  }, [spaceName, saved, goTo, t]);

  const createBot = useCallback(async () => {
    const name = botName.trim();
    if (!name) {
      setError(t("onboarding.error.bot-name"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const created = await botsCreate({
        name,
        description: botPurpose.trim() || undefined,
      });
      const progress = { ...saved, version: 1 as const, botId: created.id };
      saveProgress(progress);
      setSaved(progress);
      goTo(3);
    } catch {
      setError(t("onboarding.error.bot-create"));
    } finally {
      setBusy(false);
    }
  }, [botName, botPurpose, saved, goTo, t]);

  const connectProvider = useCallback(async () => {
    if (!provider || !apiKey.trim()) {
      setError(t("onboarding.error.key"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await modelsConnect({ provider, apiKey: apiKey.trim() });
      setApiKey("");
      const refreshed = await modelsCredentials();
      setCredentials(refreshed);
    } catch (reason: unknown) {
      // The provider's own reason is the useful one and is its to see.
      setError(
        typeof reason === "object" && reason !== null && "message" in reason && typeof (reason as { message?: unknown }).message === "string"
          ? (reason as { message: string }).message
          : t("onboarding.error.connect"),
      );
    } finally {
      setBusy(false);
    }
  }, [provider, apiKey, t]);

  const connected = (credentials?.credentials ?? []).length;
  const noProvidersReachable = catalog !== null && catalog.providers.length === 0;
  const canFinish = connected > 0 || noProvidersReachable;

  return (
    <div className="bg-background-full flex min-h-dvh flex-col items-center justify-center gap-8 px-4 py-10">
      <main className="flex w-full max-w-md flex-col gap-6">
        <header className="flex flex-col items-center gap-3 text-center">
          <RiSparkling2Line className="text-foreground-icon-primary h-7 w-7" aria-hidden="true" />
          <h1 className="text-title-3-semibold text-text-primary">{t("onboarding.title")}</h1>
          {/* Progress is deliberately visible (spec §8.2). */}
          <ol className="flex items-center gap-3" aria-label={t("onboarding.progress")}>
            {STEPS.map((entry) => {
              const reached = entry <= step;
              return (
                <li
                  key={entry}
                  aria-current={entry === step ? "step" : undefined}
                  className={`flex h-8 w-8 items-center justify-center rounded-full text-body-2-semibold ${
                    reached
                      ? "bg-background-accent-default text-text-on-accent"
                      : "bg-background-secondary-default text-text-tertiary"
                  }`}
                >
                  {entry < step ? <RiCheckLine className="h-4 w-4" aria-hidden="true" /> : entry}
                </li>
              );
            })}
          </ol>
        </header>
        <div className="border-border-button-default bg-background-primary-default flex flex-col gap-4 rounded-3xl border p-6">
          {step === 1 ? (
            <section className="flex flex-col gap-3">
              <h2 className="text-headline-medium text-text-primary">{t("onboarding.step1.title")}</h2>
              <label className="flex flex-col gap-1">
                <span className="text-text-secondary text-body-2-medium">{t("onboarding.step1.space-name")}</span>
                <input
                  type="text"
                  autoFocus
                  className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-10 rounded-md border px-3 text-body-regular placeholder:text-text-placeholder"
                  placeholder={t("onboarding.step1.space-placeholder")}
                  value={spaceName}
                  onChange={(event) => setSpaceName(event.target.value)}
                  onKeyDown={(event) => event.key === "Enter" && void createSpace()}
                />
              </label>
              {error ? <p className="text-text-error-primary text-body-2-medium" role="alert">{error}</p> : null}
              <Button variant="primary" size="medium" onClick={() => void createSpace()} disabled={busy}>
                {busy ? t("onboarding.step1.creating") : t("onboarding.step1.submit")}
              </Button>
              <Button variant="ghost" onClick={() => goTo(2)}>{t("onboarding.skip")}</Button>
            </section>
          ) : null}

          {step === 2 ? (
            <section className="flex flex-col gap-3">
              <h2 className="text-headline-medium text-text-primary">{t("onboarding.step2.title")}</h2>
              <label className="flex flex-col gap-1">
                <span className="text-text-secondary text-body-2-medium">{t("onboarding.step2.bot-name")}</span>
                <input
                  type="text"
                  autoFocus
                  className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-10 rounded-md border px-3 text-body-regular placeholder:text-text-placeholder"
                  placeholder={t("onboarding.step2.bot-placeholder")}
                  value={botName}
                  onChange={(event) => setBotName(event.target.value)}
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-text-secondary text-body-2-medium">{t("onboarding.step2.bot-purpose")}</span>
                <input
                  type="text"
                  className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-10 rounded-md border px-3 text-body-regular placeholder:text-text-placeholder"
                  placeholder={t("onboarding.step2.bot-purpose-placeholder")}
                  value={botPurpose}
                  onChange={(event) => setBotPurpose(event.target.value)}
                  onKeyDown={(event) => event.key === "Enter" && void createBot()}
                />
              </label>
              {error ? <p className="text-text-error-primary text-body-2-medium" role="alert">{error}</p> : null}
              <Button variant="primary" size="medium" onClick={() => void createBot()} disabled={busy}>
                {busy ? t("onboarding.step2.creating") : t("onboarding.step2.submit")}
              </Button>
              <Button variant="ghost" onClick={() => goTo(3)}>{t("onboarding.skip")}</Button>
            </section>
          ) : null}

          {step === 3 ? (
            <section className="flex flex-col gap-3">
              <h2 className="text-headline-medium text-text-primary">{t("onboarding.step3.title")}</h2>
              {noProvidersReachable ? (
                <p className="text-text-secondary text-body-regular rounded-xl border border-separator-border px-3 py-2">
                  {t("onboarding.step3.no-providers")}
                </p>
              ) : (
                <>
                  <label className="flex flex-col gap-1">
                    <span className="text-text-secondary text-body-2-medium">{t("onboarding.step3.provider")}</span>
                    <select
                      className="boma-focus border-border-button-default bg-background-primary-default h-10 rounded-md border px-2 text-body-regular"
                      value={provider}
                      onChange={(event) => setProvider(event.target.value)}
                    >
                      {(catalog?.providers ?? []).map((entry) => (
                        <option key={entry.id} value={entry.id}>{entry.name}</option>
                      ))}
                    </select>
                  </label>
                  <label className="flex flex-col gap-1">
                    <span className="text-text-secondary text-body-2-medium">{t("onboarding.step3.api-key")}</span>
                    <input
                      type="password"
                      autoComplete="off"
                      spellCheck="false"
                      className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-10 rounded-md border px-3 text-body-regular placeholder:text-text-placeholder"
                      placeholder="<provider key>"
                      value={apiKey}
                      onChange={(event) => setApiKey(event.target.value)}
                      onKeyDown={(event) => event.key === "Enter" && void connectProvider()}
                    />
                  </label>
                  {error ? <p className="text-text-error-primary text-body-2-medium" role="alert">{error}</p> : null}
                  <Button variant="secondary" onClick={() => void connectProvider()} disabled={busy || !provider}>
                    {busy ? t("onboarding.step3.connecting") : t("onboarding.step3.connect")}
                  </Button>
                  {connected > 0 ? (
                    <p className="text-text-primary text-body-2-medium">
                      {t("onboarding.step3.connected")} {(credentials?.credentials ?? []).map((entry) => entry.provider).join(", ")}
                    </p>
                  ) : (
                    <p className="text-text-tertiary text-body-2-medium">{t("onboarding.step3.nothing-connected")}</p>
                  )}
                </>
              )}
              <Button variant="primary" size="medium" onClick={finish} disabled={!canFinish}>
                {t("onboarding.step3.finish")}
              </Button>
              <Button variant="ghost" onClick={() => goTo(2)}>{t("onboarding.step3.back")}</Button>
            </section>
          ) : null}
        </div>
      </main>
    </div>
  );
}
