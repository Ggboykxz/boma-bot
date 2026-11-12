"use client";

import { type ReactNode, useSyncExternalStore } from "react";

import { DirectionProvider } from "@/components/foundations/direction/direction";

/** The locale the static export ships with. */
const DEFAULT_LOCALE = "en";

/**
 * The document is the single source of truth: the boot script writes `lang` and `dir` on
 * `<html>` before the first paint, and this provider reads them back. BoardUI's
 * `DirectionProvider` then gives React Aria the same locale, so keyboard navigation and
 * portalled menus follow the direction the page is actually rendered in (REQ-057).
 *
 * `useSyncExternalStore` is what keeps this honest during hydration: React renders the
 * server snapshot (`en`, exactly what the static export contains) and switches to the
 * client's locale after hydration. An effect that called `setState` would do the same, but
 * it would render twice and the lint rightly refuses it.
 *
 * The locale does not change while the page lives, so the subscription never fires.
 */
const subscribe = () => () => {};

const getClientSnapshot = () => document.documentElement.lang || DEFAULT_LOCALE;

const getServerSnapshot = () => DEFAULT_LOCALE;

export function AppProviders({ children }: { children: ReactNode }) {
  const locale = useSyncExternalStore(subscribe, getClientSnapshot, getServerSnapshot);

  return <DirectionProvider locale={locale}>{children}</DirectionProvider>;
}
