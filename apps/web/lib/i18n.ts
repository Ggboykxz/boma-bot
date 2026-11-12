/**
 * The message catalogue.
 *
 * The real catalogue is ICU-based and shared with the Rust backend; it ships for 14
 * locales from M1. This module is the client half of that pipeline and it already encodes
 * the two rules that are expensive to retrofit later:
 *
 * 1. a missing string falls back to English, never to the key and never to a blank;
 * 2. direction is derived from the locale, not from the text.
 *
 * Every user-visible string of the web client goes through `t()`. A hardcoded label is a
 * bug waiting for the locale switch.
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
    "shell.nav.primary": "Primary",
    "shell.placeholder": "The thread screen arrives with milestone M1.",
    "landing.title": "Persistent AI teammates, on hardware you own.",
    "landing.body":
      "This build carries the shell only. The thread, the run timeline and the computer panel arrive milestone by milestone; the API they talk to is already specified.",
    "landing.cta": "Open the app shell",
    "notfound.title": "Page not found",
    "notfound.body":
      "This address does not match a screen. It may have moved, or it may never have existed.",
    "notfound.back": "Back to the start",
  },
  fr: {
    "app.name": "Boma Bot",
    "shell.nav.threads": "Fils",
    "shell.nav.activity": "Activité",
    "shell.nav.primary": "Navigation principale",
    "shell.placeholder": "L'écran de conversation arrive avec le jalon M1.",
    "landing.title": "Des colocataires IA persistants, sur ta propre machine.",
    "landing.body":
      "Cette version ne contient que la coque. Le fil, la chronologie des exécutions et le panneau computer arrivent jalon par jalon ; l'API qu'ils interrogent est déjà spécifiée.",
    "landing.cta": "Ouvrir la coque de l'application",
    "notfound.title": "Page introuvable",
    "notfound.body":
      "Cette adresse ne correspond à aucun écran. Elle a peut-être été déplacée, ou n'a jamais existé.",
    "notfound.back": "Revenir au début",
  },
  he: {
    "app.name": "Boma Bot",
    "shell.nav.threads": "שיחות",
    "shell.nav.activity": "פעילות",
    "shell.nav.primary": "ניווט ראשי",
    "shell.placeholder": "מסך השיחה יגיע באבן הדרך M1.",
    "landing.title": "שותפי AI קבועים, על חומרה שבבעלותך.",
    "landing.body":
      "גרסה זו כוללת את מעטפת היישום בלבד. השיחה, ציר ההרצה ולוח המחשב יגיעו אבן דרך אחר אבן דרך; ה-API שאליו הם מתחברים כבר מוגדר.",
    "landing.cta": "פתיחת מעטפת היישום",
    "notfound.title": "הדף לא נמצא",
    "notfound.body": "הכתובת הזאת אינה מתאימה לשום מסך. אולי הוא עבר, אולי הוא מעולם לא היה.",
    "notfound.back": "חזרה להתחלה",
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
 * Returning the key (rather than an empty string) is visible in a screenshot and in a test;
 * an empty string is not.
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
