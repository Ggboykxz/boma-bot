/**
 * Shared shaping for the artifact screens (spec §8.5, REQ-036).
 *
 * Sizes and types are formatted here so every screen reads the same way. The bytes themselves
 * are never inspected: `mime_type` is what the server proved, the declared MIME is only a hint.
 */

export const MIME_IMAGE = /^image\//;
export const MIME_PDF = /^application\/pdf/;
export const MIME_TEXT = /^text\/|json|xml|csv|markdown/;

/** The human format of a byte count: 1 KiB steps, 2 significant figures. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return "—";
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KiB", "MiB", "GiB", "TiB"] as const;
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

/** The coarse bucket the filters work over. */
export function typeBucket(mimeType: string): "image" | "pdf" | "text" | "other" {
  const mime = mimeType.toLowerCase();
  if (MIME_IMAGE.test(mime)) {
    return "image";
  }
  if (MIME_PDF.test(mime)) {
    return "pdf";
  }
  if (MIME_TEXT.test(mime)) {
    return "text";
  }
  return "other";
}

/** A short human label for a moment; `<time>` carries the full one. */
export function shortDate(iso: string): string {
  const time = Date.parse(iso);
  if (Number.isNaN(time)) {
    return iso;
  }
  const date = new Date(time);
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

/** Truncate with an ellipsis; never more than `max` bytes of UTF-8 output. */
export function clip(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, max)}…`;
}
