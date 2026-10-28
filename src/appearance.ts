import { findTheme, themePresets, type ThemeId } from './theme-presets';

export type AppearancePreferences = Readonly<{
  version: 1;
  application: ThemeId;
  terminal: ThemeId | null;
}>;
const defaults: AppearancePreferences = Object.freeze({
  version: 1,
  application: 'original',
  terminal: null,
});
const key = 'ongleet.appearance';
export function createAppearanceStore(storage: () => Pick<Storage, 'getItem' | 'setItem'>) {
  let preferences = defaults;
  let persistenceNotice = '';
  const listeners = new Set<() => void>();
  try {
    const raw = storage().getItem(key);
    const value = raw ? JSON.parse(raw) : null;
    if (value?.version === 1)
      preferences = Object.freeze({
        version: 1,
        application: findTheme(value.application)?.id ?? 'original',
        terminal: findTheme(value.terminal)?.id ?? null,
      });
  } catch {
    persistenceNotice =
      'Appearance preferences could not be restored. Changes remain available in memory.';
  }
  const snapshot = () =>
    Object.freeze({
      preferences,
      application: findTheme(preferences.application)!,
      terminal: findTheme(preferences.terminal ?? preferences.application)!,
      persistenceNotice,
    });
  let current = snapshot();
  return {
    getSnapshot: () => current,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    update(update: Partial<Pick<AppearancePreferences, 'application' | 'terminal'>>) {
      preferences = Object.freeze({
        version: 1,
        application: findTheme(update.application ?? preferences.application)?.id ?? 'original',
        terminal:
          update.terminal === undefined
            ? preferences.terminal
            : (findTheme(update.terminal)?.id ?? null),
      });
      try {
        storage().setItem(key, JSON.stringify(preferences));
        persistenceNotice = '';
      } catch {
        persistenceNotice =
          'Appearance changes could not be saved. They will be lost when the app restarts.';
      }
      current = snapshot();
      listeners.forEach((listener) => listener());
    },
  };
}
export const appearance = createAppearanceStore(() => window.localStorage);
export const themeAccess = Object.freeze({
  getSnapshot: appearance.getSnapshot,
  subscribe: appearance.subscribe,
});
export function applyAppearance() {
  const theme = appearance.getSnapshot().application;
  Object.entries(theme.ui).forEach(([key, value]) =>
    document.documentElement.style.setProperty(`--${key}`, value),
  );
  document.documentElement.style.colorScheme = theme.mode;
}
export { themePresets };
