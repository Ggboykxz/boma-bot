"use client";

import { botsList, type RosterBotView } from "@/lib/api/rpc";

/**
 * The server roster as a tiny external store.
 *
 * The rail subscribes with `useSyncExternalStore`, and actions call `refreshRoster()` — the
 * fetch itself never runs inside a React render, so the screen has one source of truth and no
 * cascading state. Failures keep the last good answer; a later call decides the next one.
 */

const listeners = new Set<() => void>();
let roster: readonly RosterBotView[] | null = null;
const SERVER_ROSTER: readonly RosterBotView[] = [];

/** The roster the server last gave, or null until the first answer arrives. */
export function listRoster(): readonly RosterBotView[] | null {
  return roster;
}

/** The snapshot the server renders: empty until a client store exists. */
export function serverRoster(): readonly RosterBotView[] {
  return SERVER_ROSTER;
}

/** Subscribes to roster changes. */
export function subscribeRoster(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Fetches the roster from the server and publishes it. */
export async function refreshRoster(): Promise<void> {
  try {
    const reply = await botsList();
    roster = reply.bots;
    for (const listener of listeners) {
      listener();
    }
  } catch {
    // The last good roster stays on screen; the next call decides the next one.
  }
}
