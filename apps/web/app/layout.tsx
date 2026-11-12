import type { Metadata, Viewport } from "next";
import { Inter } from "next/font/google";
import type { ReactNode } from "react";

import "@/styles/globals.css";
import "@/styles/boma.css";

import { AppProviders } from "@/components/providers/app-providers";

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
 * The theme reads its typeface from `--font-inter`, so the font has to be loaded and the
 * variable has to reach `<html>`. `next/font` self-hosts the file: no request leaves the
 * machine at runtime.
 */
const inter = Inter({
  variable: "--font-inter",
  subsets: ["latin"],
  axes: ["opsz"],
  display: "swap",
});

/**
 * Applies the theme and the locale before the first paint.
 *
 * The static export always ships the light theme and the English locale, so the browser
 * decides: an explicit choice in `localStorage` wins, the operating system preference is
 * the fallback. The script runs while the document is parsed, so there is no flash of the
 * wrong theme (Next.js guide "Preventing flash before hydration").
 *
 * `lang` and `dir` are set together: the locale decides the direction, never the text.
 */
const BOOT_SCRIPT = `(function(){try{
var s=localStorage.getItem("boma-theme");
var d=s?s==="dark":window.matchMedia("(prefers-color-scheme: dark)").matches;
document.documentElement.classList.toggle("dark",d);
var l=localStorage.getItem("boma-locale");
if(l){document.documentElement.lang=l;document.documentElement.dir=/^(ar|fa|he|ur)([-_]|$)/i.test(l)?"rtl":"ltr";}
}catch(e){}})();`;

/**
 * The document shell.
 *
 * `lang` and `dir` are set from the catalogue, not guessed from the content: the direction
 * provider reads them and keeps React Aria's keyboard behaviour and portalled menus in
 * sync (REQ-057). Both attributes are present now so a screen reader and the axe gate
 * exercise the same markup the product will ship.
 *
 * `suppressHydrationWarning` is on `<html>` because the boot script changes attributes
 * React did not render. React compares the attributes of that one element and warns
 * otherwise; the change is intentional, so the warning would be noise.
 */
export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" dir="ltr" suppressHydrationWarning className={inter.variable}>
      <head>
        <script dangerouslySetInnerHTML={{ __html: BOOT_SCRIPT }} />
      </head>
      <body className="bg-background-full text-text-primary text-body-medium">
        <AppProviders>{children}</AppProviders>
      </body>
    </html>
  );
}
