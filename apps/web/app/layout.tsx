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
 * The document shell.
 *
 * `lang` and `dir` are set from the catalogue, not guessed from the content: the DirectionProvider
 * from BoardUI replaces the hard-coded `ltr` in M1, once a locale can be chosen at runtime
 * (REQ-057). Both attributes are present now so a screen reader and the axe gate exercise the
 * same markup the product will ship.
 */
export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" dir="ltr">
      <body className="bg-background-default text-text-primary text-body-medium">{children}</body>
    </html>
  );
}
