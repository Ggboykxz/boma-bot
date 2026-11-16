"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useState, useSyncExternalStore } from "react";
import {
  RiArrowLeftLine,
  RiCheckLine,
  RiClipboardLine,
  RiDownloadLine,
  RiErrorWarningLine,
  RiMessage2Line,
  RiQuestionLine,
  RiDeleteBinLine,
} from "@remixicon/react";

import { Button } from "@/components/base/buttons/button";
import {
  artifactsGet,
  artifactsListVersions,
  artifactsRemove,
  type ArtifactGetReply,
  type ArtifactVersionView,
} from "@/lib/api/rpc";
import { locSubscribe, match } from "@/lib/loc";
import { clip, formatBytes, shortDate, typeBucket } from "@/components/artifacts/artifacts-shared";
import { useTranslation } from "@/lib/use-translation";

type Phase = "loading" | "ready" | "unavailable" | "error";

const MAX_INLINE_BYTES = 4 * 1024 * 1024;

/** Artifact bytes are untrusted; these helpers never make them markup. */
function bytesFromBase64(base64: string): Uint8Array<ArrayBuffer> {
  const binary = atob(base64);
  const bytes = new Uint8Array(new ArrayBuffer(binary.length));
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

function textPreview(base64: string): string | null {
  let bytes: Uint8Array;
  try {
    bytes = bytesFromBase64(base64);
  } catch {
    return null;
  }
  if (bytes.length > MAX_INLINE_BYTES) {
    return null;
  }
  const preview = new TextDecoder("utf-8", { fatal: false }).decode(bytes.subarray(0, 128 * 1024));
  return preview;
}

/**
 * `/app/artifacts/<artifactId>` (spec §8.5, REQ-036). The export carries one shell for the
 * `/app/artifacts/` subtree (ADR-013): the id is read from the URL, never baked in.
 *
 * Never renders HTML or arbitrary markup from the artifact; images and PDFs get a bounded
 * frame fed by a blob URL, text gets `<pre>`, everything else is a download away.
 */
export function ArtifactViewer() {
  const t = useTranslation();
  const router = useRouter();
  const artifactId = useSyncExternalStore(locSubscribe, () => match("/app/artifacts/", window.location.pathname), () => null);

  const [phase, setPhase] = useState<Phase>("loading");
  const [data, setData] = useState<ArtifactGetReply | null>(null);
  const [versions, setVersions] = useState<ArtifactVersionView[]>([]);
  const [pinnedVersion, setPinnedVersion] = useState<number | undefined>(undefined);
  const [copied, setCopied] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState(false);
  const dataUrl = data ? `data:${data.artifact.mimeType};base64,${data.contentBase64}` : null;

  useEffect(() => {
    if (!artifactId) {
      return;
    }
    const controller = new AbortController();
    Promise.all([
      artifactsGet({ artifactId, version: pinnedVersion }, controller.signal),
      artifactsListVersions({ artifactId }, controller.signal),
    ])
      .then(([body, list]) => {
        if (controller.signal.aborted) {
          return;
        }
        setData(body);
        setVersions(list.versions);
        setPhase("ready");
      })
      .catch(() => {
        if (!controller.signal.aborted) {
          setPhase("error");
        }
      });
    return () => controller.abort();
  }, [artifactId, pinnedVersion]);

  // The data URL travels with the renderer; no lifetime to manage. The bytes are never
  // re-encoded as markup — the attribute is a blob reference, not HTML.
  if (artifactId === null) {
    // The URL names neither a home nor an artifact — the grid it came from owns that route.
    return (
      <section className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <RiQuestionLine className="text-foreground-icon-secondary h-8 w-8" aria-hidden="true" />
        <p className="text-text-primary text-body-regular">{t("artifacts.detail.pick-one")}</p>
        <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary" href="/app/artifacts/">
          {t("artifacts.detail.back-to-grid")}
        </Link>
      </section>
    );
  }

  if (phase === "loading") {
    return (
      <section className="flex h-full flex-col gap-4 p-4 sm:p-6" aria-busy="true">
        <div className="bg-background-secondary-default h-8 w-64 max-w-full rounded-md" />
        <div className="bg-background-secondary-default h-96 w-full rounded-2xl" />
      </section>
    );
  }

  if (phase === "error" || phase === "unavailable" || !data) {
    return (
      <section className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <RiErrorWarningLine className="text-foreground-icon-primary h-8 w-8" aria-hidden="true" />
        <p className="text-text-primary text-body-regular">{t("artifacts.error.load")}</p>
        <Link className="boma-target boma-focus text-text-secondary hover:text-text-primary" href="/app/artifacts/">
          {t("artifacts.detail.back-to-grid")}
        </Link>
      </section>
    );
  }

  const { artifact } = data;
  const bucket = typeBucket(artifact.mimeType);
  const textContent = textPreview(data.contentBase64);
  const canInline = textContent !== null;
  const versionsSorted = [...versions].sort((a, b) => b.version - a.version);

  const copyContent = async () => {
    const payload = textContent ?? artifact.hash;
    try {
      await navigator.clipboard.writeText(payload);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      // The clipboard is denied outside a secure context; the visible label stays honest.
      setCopied(false);
    }
  };

  const download = () => {
    if (!dataUrl) {
      return;
    }
    const anchor = document.createElement("a");
    anchor.href = dataUrl;
    anchor.download = artifact.name;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
  };

  const doDelete = async () => {
    setDeleting(true);
    setDeleteError(false);
    try {
      await artifactsRemove({ artifactId: artifact.rootArtifactId });
      router.replace("/app/artifacts/");
    } catch {
      setDeleting(false);
      setDeleteError(true);
    }
  };

  return (
    <section className="flex h-full min-w-0 flex-col gap-4 overflow-y-auto p-4 sm:p-6">
      <nav className="text-text-tertiary text-body-2-medium">
        <Link href="/app/artifacts/" className="boma-target boma-focus hover:text-text-secondary inline-flex items-center gap-1 rounded-md">
          <RiArrowLeftLine className="h-4 w-4" aria-hidden="true" />
          {t("artifacts.detail.back-to-grid")}
        </Link>
      </nav>

      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-1">
          <h1 className="text-title-3-semibold text-text-primary break-words">{artifact.name}</h1>
          <p className="text-text-secondary text-body-2-medium">
            {formatBytes(artifact.size)} · {artifact.mimeType} · sha256 {artifact.hash.slice(0, 15)}…
          </p>
          <p className="text-text-tertiary text-body-2-medium">
            <time dateTime={artifact.createdAt}>{shortDate(artifact.createdAt)}</time>
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <select
            aria-label={t("artifacts.detail.version")}
            value={artifact.version}
            onChange={(event) => setPinnedVersion(Number(event.target.value))}
            className="boma-focus border-border-button-default bg-background-primary-default h-9 rounded-md border px-2 text-body-regular"
          >
            {versionsSorted.map((entry) => (
              <option key={entry.id} value={entry.version}>
                {t("artifacts.detail.version")} {entry.version}
              </option>
            ))}
          </select>
          <Button variant="secondary" onClick={download} disabled={!dataUrl}>
            <RiDownloadLine className="h-4 w-4" aria-hidden="true" />
            {t("artifacts.detail.download")}
          </Button>
          <Button variant="secondary" onClick={() => void copyContent()}>
            {copied ? <RiCheckLine className="h-4 w-4" aria-hidden="true" /> : <RiClipboardLine className="h-4 w-4" aria-hidden="true" />}
            {copied ? t("artifacts.detail.copied") : t("artifacts.detail.copy")}
          </Button>
          {artifact.botId ? (
            <Link href={`/app/${artifact.botId}/`} className="boma-focus inline-flex no-underline">
              <Button variant="secondary">
                <RiMessage2Line className="h-4 w-4" aria-hidden="true" />
                {t("artifacts.detail.open-in-thread")}
              </Button>
            </Link>
          ) : null}
          {confirmDelete ? (
            <span className="inline-flex items-center gap-2">
              <Button variant="danger" type="button" onClick={() => void doDelete()} disabled={deleting}>
                {deleting ? t("artifacts.detail.deleting") : t("artifacts.detail.delete-confirm")}
              </Button>
              <Button variant="ghost" type="button" onClick={() => setConfirmDelete(false)}>
                {t("common.cancel")}
              </Button>
            </span>
          ) : (
            <Button variant="ghost" type="button" onClick={() => setConfirmDelete(true)}>
              <RiDeleteBinLine className="h-4 w-4" aria-hidden="true" />
              {t("artifacts.detail.delete")}
            </Button>
          )}
        </div>
      </header>
      {deleteError ? <p className="text-text-error-primary text-body-2-medium" role="alert">{t("artifacts.error.delete")}</p> : null}

      {bucket === "image" && dataUrl ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={dataUrl}
          alt={clip(artifact.name, 128)}
          className="max-h-[60vh] max-w-full rounded-2xl border border-border-button-default object-contain"
        />
      ) : bucket === "pdf" && dataUrl ? (
        <embed src={dataUrl} type="application/pdf" className="h-[70vh] w-full rounded-2xl border border-border-button-default" aria-label={artifact.name} />
      ) : canInline ? (
        <pre className="text-text-primary border-border-button-default bg-background-secondary-default max-h-[70vh] overflow-auto rounded-2xl border p-4 text-body-2-regular whitespace-pre-wrap break-words">{textContent}</pre>
      ) : (
        <p className="text-text-tertiary text-body-regular">{t("artifacts.detail.download-only")}</p>
      )}
    </section>
  );
}
