/**
 * The smallest possible slice of the message catalogue (REQ-057).
 *
 * The real catalogue is ICU-based, shared with the Rust backend and shipped for 14 locales from
 * M1. This module exists now so that the two rules which are easy to break later are already
 * encoded and tested:
 *
 * 1. a missing string falls back to English, never to the key and never to a blank;
 * 2. direction is derived from the locale, not from the text.
 */

export type Locale = "en" | "fr" | "he";

export type Direction = "ltr" | "rtl";

/** Locales the M0 shell can render. `he` is here as the RTL proof, not as a shipped translation. */
export const SUPPORTED_LOCALES: readonly Locale[] = ["en", "fr", "he"] as const;

const CATALOGUES: Record<Locale, Readonly<Record<string, string>>> = {
  en: {
    "app.name": "Boma Bot",
    "shell.nav.threads": "Threads",
    "shell.nav.activity": "Activity",
    "shell.nav.settings": "Settings",
    "shell.placeholder": "The thread screen arrives with milestone M1.",
  },
  fr: {
    "app.name": "Boma Bot",
    "shell.nav.threads": "Fils",
    "shell.nav.activity": "Activité",
    "shell.placeholder": "L'écran de conversation arrive avec le jalon M1.",
  },
  he: {
    "app.name": "Boma Bot",
    "shell.nav.threads": "שיחות",
    "shell.nav.activity": "פעילות",
    "shell.placeholder": "מסך השיחה יגיע באבן הדרך M1.",
  },
};

const RTL_LANGUAGES = new Set(["ar", "fa", "he", "ur"]);

/** The language subtag of a BCP 47 tag, lowercased: `he-IL` becomes `he`. */
export function languageOf(locale: string): string {
  return locale.split("-")[0]?.toLowerCase() ?? "";
}

/** Whether a locale is written right to left. */
export function direction(locale: string): Direction {
  return RTL_LANGUAGES.has(languageOf(locale)) ? "rtl" : "ltr";
}

/** Whether this build has a catalogue for the locale. */
export function isSupported(locale: string): locale is Locale {
  return (SUPPORTED_LOCALES as readonly string[]).includes(locale);
}

/**
 * Looks a message up, falling back to English and then to the key itself.
 *
 * Returning the key (rather than an empty string) is visible in a screenshot and in a test; an
 * empty string is not.
 */
export function t(locale: string, key: string): string {
  const language = languageOf(locale);
  if (isSupported(language)) {
    const translated = CATALOGUES[language][key];
    if (translated !== undefined) {
      return translated;
    }
  }
  return CATALOGUES.en[key] ?? key;
}
