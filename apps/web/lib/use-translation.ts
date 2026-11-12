"use client";

import { useLocale } from "react-aria-components";

import { t } from "./i18n";

/**
 * The translator for the active locale.
 *
 * `useLocale()` comes from the direction provider, so the catalogue lookup and the
 * direction always agree: a Hebrew page translates with the Hebrew catalogue and lays
 * out right to left (REQ-057).
 */
export function useTranslation(): (key: string) => string {
  const { locale } = useLocale();
  return (key: string) => t(locale, key);
}
