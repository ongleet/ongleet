import { useSyncExternalStore } from 'react';
import { fr } from './locales/fr';

export type Locale = 'en' | 'fr';
const storageKey = 'ongleet.locale';
const listeners = new Set<() => void>();
let locale: Locale = 'en';
let persistenceNotice = '';
try {
  const saved = localStorage.getItem(storageKey);
  if (saved === 'en' || saved === 'fr') locale = saved;
} catch {
  // Storage restrictions must not prevent changing the language in memory.
}

export function getLocale(): Locale {
  return locale;
}
export function subscribeLocale(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
export function useLocale(): Locale {
  return useSyncExternalStore(subscribeLocale, getLocale);
}
export function setLocale(next: Locale): void {
  locale = next;
  persistenceNotice = '';
  try {
    localStorage.setItem(storageKey, next);
  } catch {
    persistenceNotice = 'Language preference could not be saved.';
  }
  document.documentElement.lang = next;
  for (const listener of listeners) listener();
}
export function localePersistenceNotice(): string {
  return persistenceNotice ? t(persistenceNotice) : '';
}
/** Translate UI copy only; never pass user content, paths or protocol values. */
export function t(message: string, values: Record<string, string | number> = {}): string {
  const translated = locale === 'fr' ? (fr[message] ?? message) : message;
  return translated.replace(/\{(\w+)\}/g, (match, key: string) =>
    Object.hasOwn(values, key) ? String(values[key]) : match,
  );
}
if (typeof document !== 'undefined') document.documentElement.lang = locale;

/** CodeMirror uses its own phrase facet for search controls and announcements. */
export function editorPhrases(): Record<string, string> {
  return locale === 'fr' ? fr : {};
}
