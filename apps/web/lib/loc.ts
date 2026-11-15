/**
 * Reads one opaque path segment from the current location.
 *
 * The static export carries one shell per screen family (ADR-013): `/p/<token>` and
 * `/app/artifacts/<artifactId>` are pre-rendered with a placeholder param, and the real value is
 * whatever the location says. This helper exists so every screen reads the URL the same way.
 *
 * The answer is `null` when the path does not match — the screen shows its empty state.
 */
const subscribe = () => () => {};

export function pathSegment(prefix: string): string | null {
  const path =
    typeof window === "undefined" ? "" : window.location.pathname;
  return match(prefix, path);
}

/** The server-side render never reads a path, so the hook renders `null` first. */
export const locSubscribe = subscribe;

/** The same read, exported as a pure function for the hook and for tests. */
export function match(prefix: string, path: string): string | null {
  // Accept both `/p/token` and `/p/token/` (the export is directory-style).
  const match = new RegExp(`^${escape(prefix)}([^/]+)/?$`).exec(path);
  if (!match?.[1]) {
    return null;
  }
  try {
    return decodeURIComponent(match[1]);
  } catch {
    return null;
  }
}

function escape(prefix: string): string {
  return prefix.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
