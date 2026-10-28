// Run before importing consumers: editor settings are read at module initialization.
export function migrateBrandingStorage(storage: Storage): void {
  const copies: [string, string, string][] = [];
  for (let index = 0; index < storage.length; index += 1) {
    const key = storage.key(index);
    if (
      key &&
      (key === 'devcenter.editor.wrap' ||
        key === 'devcenter.sidebarWidth' ||
        key.startsWith('devcenter.editor.draft:') ||
        key.startsWith('devcenter.pr-reviews.v1:'))
    ) {
      const value = storage.getItem(key);
      const target = `ongleet${key.slice('devcenter'.length)}`;
      const existing = storage.getItem(target);
      if (value !== null) {
        if (existing !== null && existing !== value) {
          throw new Error('Conflicting Ongleet browser storage; original keys retained');
        }
        copies.push([key, target, value]);
      }
    }
  }
  // Copy everything before removing anything. Quota failures remain retryable.
  for (const [, target, value] of copies) {
    storage.setItem(target, value);
    if (storage.getItem(target) !== value) throw new Error('Browser storage migration failed');
  }
  for (const [source] of copies) storage.removeItem(source);
}

// Unavailable storage must not prevent in-memory preferences or editor use.
let browserStorage: Storage | undefined;
try {
  browserStorage = window.localStorage;
  browserStorage.getItem('ongleet.appearance');
} catch {
  browserStorage = undefined;
}
try {
  if (browserStorage) migrateBrandingStorage(browserStorage);
} catch (error) {
  // Do not initialize consumers with incomplete state; retry after resolving storage.
  document.body.textContent = `Ongleet storage migration could not complete. ${String(error)}`;
  throw error;
}
