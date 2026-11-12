"use client";

import { useSyncExternalStore } from "react";

import { ThreadsHome } from "@/components/shell/threads-home";
import { ThreadScreen } from "@/components/thread/thread-screen";
import { botIdFromPath } from "@/lib/bot-directory";

/**
 * The entry of the `/app` subtree.
 *
 * The static export carries one shell for the whole application: the Rust static handler maps an
 * unknown path under `/app` to the application's `index.html` (ADR-013). That shell has to know
 * which screen the URL asks for, so it reads the path.
 *
 * The first paint is the home — the server cannot know the path — and the screen the URL names
 * takes over as soon as the document is alive. In development the dynamic route answers
 * `/app/<botId>/` directly, and this dispatch never runs.
 */
/** The path never changes without a reload in the static export, so there is nothing to subscribe to. */
const subscribe = () => () => {};

export function AppEntry() {
  // The server cannot know the path, so the first paint is the home; the URL names the thread
  // once the document is alive.
  const botId = useSyncExternalStore(subscribe, () => botIdFromPath(), () => null);

  if (botId === null) {
    return <ThreadsHome />;
  }

  return <ThreadScreen botId={botId} />;
}
