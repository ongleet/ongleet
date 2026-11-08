import { appearance } from './appearance';
import { getLocale, setLocale, type Locale } from './i18n';
import type { ThemeId } from './theme-presets';

export type ApplicationAppearance = {
  application: ThemeId;
  terminal: ThemeId | null;
  language: Locale;
};
export const defaultApplicationAppearance: ApplicationAppearance = {
  application: 'original',
  terminal: null,
  language: 'en',
};
export function currentApplicationAppearance(): ApplicationAppearance {
  const { application, terminal } = appearance.getSnapshot().preferences;
  return { application, terminal, language: getLocale() };
}
export function applyApplicationAppearance(settings: ApplicationAppearance): void {
  appearance.update({ application: settings.application, terminal: settings.terminal });
  setLocale(settings.language);
}
