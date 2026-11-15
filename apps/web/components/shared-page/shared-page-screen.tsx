"use client";

import { useEffect, useState } from "react";

/**
 * The blocks a page may carry (mirror of the backend's `PageBlock`; the validator is the
 * backend's job, this renderer stays safe even against a hand-written payload).
 */
export type SharedPageBlock =
  | { type: "text"; text: string }
  | { type: "table"; columns: string[]; rows: string[][] }
  | { type: "chart"; label: string; points: Array<{ label: string; value: number }> }
  | { type: "link"; label: string; href: string }
  | { type: "image"; src: string; alt: string };

interface SharedPage {
  title: string;
  blocks: SharedPageBlock[];
}

type Phase = "loading" | "unavailable" | "ready";

const COLUMNS_CAP = 16;
const ROWS_CAP = 64;
const POINTS_CAP = 64;
const TEXT_CAP = 4096;

function safeLink(href: string): string | null {
  const trimmed = href.trim();
  // The backend admits https?: and mailto: only, and its tests pin that; this is a second wall.
  return /^(https?:\/\/|mailto:)[^\s]+$/i.test(trimmed) ? trimmed : null;
}

function safeImage(src: string): string | null {
  const trimmed = src.trim();
  // The backend admits https: and data:image/(png|jpeg|gif|webp);base64 only.
  if (/^https:\/\/[^\s]+$/i.test(trimmed)) {
    return trimmed;
  }
  if (/^data:image\/(png|jpeg|gif|webp);base64,[A-Za-z0-9+/=]+$/i.test(trimmed)) {
    return trimmed;
  }
  return null;
}

function clampText(text: string): string {
  return text.length > TEXT_CAP ? `${text.slice(0, TEXT_CAP)}…` : text;
}

/**
 * The text blocks of one public page; tables, a bar chart and links render from their values
 * with no script anywhere: the page is data, never markup.
 */
function Block({ block }: { block: SharedPageBlock }) {
  switch (block.type) {
    case "text":
      return <p className="text-text-primary text-body-regular max-w-prose whitespace-pre-wrap break-words">{clampText(block.text)}</p>;
    case "table": {
      const columns = block.columns.slice(0, COLUMNS_CAP);
      const rows = block.rows.slice(0, ROWS_CAP);
      return (
        <div className="overflow-x-auto">
          <table className="text-text-primary border-border-table w-full border-collapse text-body-regular">
            <thead>
              <tr className="text-start">
                {columns.map((column) => (
                  <th key={column} scope="col" className="border-separator-border border-b px-3 py-2 text-start font-medium">{column}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, index) => (
                <tr key={index}>
                  {columns.map((_, cellIndex) => (
                    <td key={cellIndex} className="border-separator-border border-b px-3 py-2 align-top">{row[cellIndex] ?? ""}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    }
    case "chart": {
      const points = block.points.slice(0, POINTS_CAP);
      const max = points.reduce((highest, point) => Math.max(highest, point.value), 0);
      const top = max > 0 ? max : 1;
      return (
        <figure className="flex flex-col gap-2">
          <figcaption className="text-text-secondary text-body-2-medium">{block.label}</figcaption>
          <svg viewBox={`0 0 ${points.length * 48 + 8} 140`} role="img" aria-label={block.label} className="w-full max-w-md">
            {points.map((point, index) => {
              const height = Math.max((point.value / top) * 100, 2);
              const x = index * 48 + 4;
              const y = 120 - height;
              return (
                <rect key={`${point.label}-${index}`} x={x} y={y} width="36" height={height} rx="2" className="fill-accent-400">
                  <title>{`${point.label}: ${point.value}`}</title>
                </rect>
              );
            })}
            <line x1="0" y1="122" x2="100%" y2="122" className="stroke-separator-border" strokeWidth="1" />
          </svg>
        </figure>
      );
    }
    case "link": {
      const href = safeLink(block.href);
      if (!href) {
        return <p className="text-text-primary text-body-regular">{block.label}</p>;
      }
      return (
        <a className="boma-focus accent-600 hover:accent-700 inline-flex text-body-medium underline underline-offset-2" href={href} target="_blank" rel="noopener noreferrer nofollow">
          {block.label}
        </a>
      );
    }
    case "image": {
      const src = safeImage(block.src);
      if (!src) {
        return <p className="text-text-primary text-body-regular">{block.alt}</p>;
      }
      // eslint-disable-next-line @next/next/no-img-element
      return <img src={src} alt={block.alt} className="max-w-full rounded-xl" loading="lazy" />;
    }
    default: {
      const unreachable: never = block;
      return unreachable;
    }
  }
}

/**
 * The public screen for a shared page. The request reaches the API without a session; a 404
 * from the token path and a missing page share the same document, always.
 */
export function SharedPageScreen({ token }: { token: string }) {
  const [phase, setPhase] = useState<Phase>("loading");
  const [page, setPage] = useState<SharedPage | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    fetch(`/api/v1/pages/shared/${encodeURIComponent(token)}`, {
      headers: { accept: "application/json" },
      credentials: "omit",
      signal: controller.signal,
    })
      .then(async (response) => {
        if (!response.ok) {
          // 404, 410, anything else: the same "not available" screen, same words, same shape.
          setPhase("unavailable");
          return;
        }
        const body = (await response.json()) as SharedPage;
        if (controller.signal.aborted) {
          return;
        }
        document.title = body.title;
        setPage(body);
        setPhase("ready");
      })
      .catch(() => {
        if (!controller.signal.aborted) {
          setPhase("unavailable");
        }
      });

    return () => {
      controller.abort();
    };
  }, [token]);

  if (phase === "unavailable") {
    // Indistinguishable from a valid page: spec/08-ui-ux.md §8.8 requires the same document.
    return (
      <main className="bg-background-full flex min-h-dvh flex-col items-center justify-center gap-3 px-6 text-center">
        <h1 className="text-title-2-medium text-text-primary">Page not available</h1>
        <p className="text-text-secondary text-body-regular max-w-prose">
          This link does not match a page that exists. It may have been revoked, or it may never have existed.
        </p>
      </main>
    );
  }

  if (phase === "loading") {
    return (
      <main className="bg-background-full flex min-h-dvh flex-col items-center justify-center gap-4 px-6" aria-busy="true" aria-label="Loading the page">
        <div className="bg-background-secondary-default h-8 w-64 rounded-md" />
        <div className="bg-background-secondary-default h-4 w-96 max-w-full rounded-md" />
        <div className="bg-background-secondary-default h-4 w-80 max-w-full rounded-md" />
      </main>
    );
  }

  return (
    <main className="bg-background-full min-h-dvh">
      <article className="mx-auto flex w-full max-w-3xl flex-col gap-6 px-4 py-10 sm:px-6">
        <h1 className="text-title-2-medium text-text-primary break-words">{page?.title}</h1>
        {page?.blocks.map((block, index) => (
          <Block key={`${block.type}-${index}`} block={block} />
        ))}
      </article>
    </main>
  );
}
