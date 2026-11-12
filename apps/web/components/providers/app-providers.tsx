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
// A real subscription is the contract of useSyncExternalStore: without one, React never learns
// that the document's locale differs from the server snapshot it hydrated with, and the page
// stays in English forever. The mutation fires when the boot script (or a future locale switcher)
// sets `lang` on the document.
// The subscription lives at module scope: a new identity every render would make React
// resubscribe forever. React calls it once per mount; the store itself is the document, which
// is stable for the lifetime of the export.
const subscribe = (listener: () => void) => {
  // The document is already `he` when React mounts: the boot script ran before the bundle did.
  // Asking React to re-read the source right after the subscription exists is what performs the
  // switch from the hydrated `en` to the document's real locale; the observer then keeps the
  // two in step if the attribute ever changes again.
  queueMicrotask(listener);
  const observer = new MutationObserver(listener);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["lang"] });
  return () => observer.disconnect();
};

const getClientSnapshot = () => document.documentElement.lang || DEFAULT_LOCALE;

const getServerSnapshot = () => DEFAULT_LOCALE;

export function AppProviders({ children }: { children: ReactNode }) {
  const locale = useSyncExternalStore(subscribe, getClientSnapshot, getServerSnapshot);

  return <DirectionProvider locale={locale}>{children}</DirectionProvider>;
}
