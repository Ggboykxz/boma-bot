/**
 * The bots the client knows about.
 *
 * The wire contract has no `bots.list` procedure, so the rail cannot ask the server for the
 * roster. It shows the bots the client has already met: the thread screen registers the bot it
 * just opened, and the rail renders what has been registered.
 *
 * The store is a tiny external store rather than a context: it is written from one place and
 * read from another, and `useSyncExternalStore` keeps the two in step without a provider.
 */

export interface BotSummary {
  id: string;
  name: string;
}

const EMPTY: readonly BotSummary[] = [];

let bots: readonly BotSummary[] = EMPTY;
const listeners = new Set<() => void>();

/** Remembers a bot, or renames the one that is already known. */
export function registerBot(bot: BotSummary): void {
  const known = bots.some((entry) => entry.id === bot.id);
  bots = known
    ? bots.map((entry) => (entry.id === bot.id ? bot : entry))
    : [...bots, bot];
  for (const listener of listeners) {
    listener();
  }
}

/** The bots the client has met. */
export function listBots(): readonly BotSummary[] {
  return bots;
}

/** Subscribes to the roster. */
export function subscribeBots(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The snapshot the server renders: nothing, because the server never met a bot. */
export function serverBots(): readonly BotSummary[] {
  return EMPTY;
}

/**
 * The bot's id, read from the path.
 *
 * The static export carries one shell for the whole `/app` subtree: the Rust static handler maps
 * an unknown path under `/app` to the application's `index.html` (ADR-013). The screen therefore
 * reads the id from the URL rather than from the route param, which the export cannot know.
 *
 * The answer is `null` when the path is the home or the activity view.
 */
export function botIdFromPath(pathname?: string): string | null {
  const path = pathname ?? (typeof window === "undefined" ? "" : window.location.pathname);
  const match = /^\/app\/([^/]+)\/?$/.exec(path);
  if (!match?.[1]) {
    return null;
  }
  return decodeURIComponent(match[1]);
}
