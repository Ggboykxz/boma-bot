import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";

import "./globals.css";

export const metadata: Metadata = {
  title: "Boma Bot",
  description: "Self-hosted AI teammates with a persistent thread, memory and a computer.",
  applicationName: "Boma Bot",
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  colorScheme: "light dark",
};

/**
 * Applies the theme before the first paint.
 *
 * The static export always ships the light theme, so the browser decides: an explicit choice in
 * `localStorage` wins, the operating system preference is the fallback. The script runs while the
 * document is parsed, so there is no flash of the wrong theme (Next.js guide "Preventing flash
 * before hydration").
 */
const THEME_SCRIPT = `(function(){try{var s=localStorage.getItem("boma-theme");var d=s?s==="dark":window.matchMedia("(prefers-color-scheme: dark)").matches;document.documentElement.classList.toggle("dark",d)}catch(e){}})();`;

/**
 * The document shell.
 *
 * `lang` and `dir` are set from the catalogue, not guessed from the content: the DirectionProvider
 * from BoardUI replaces the hard-coded `ltr` in M1, once a locale can be chosen at runtime
 * (REQ-057). Both attributes are present now so a screen reader and the axe gate exercise the
 * same markup the product will ship.
 *
 * `suppressHydrationWarning` is on `<html>` because the theme script and the locale switch change
 * attributes React did not render. React compares the attributes of that one element and warns
 * otherwise; the change is intentional, so the warning would be noise (Next.js guide "Preventing
 * flash before hydration").
 */
export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" dir="ltr" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: THEME_SCRIPT }} />
      </head>
      <body className="bg-background-default text-text-primary text-body-medium">{children}</body>
    </html>
  );
}
