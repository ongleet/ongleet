import { useEffect, useState } from 'react';

const storageKey = 'ongleet.editor.wrap';
const changeEvent = 'ongleet-editor-settings-changed';
let wrap = false;
try {
  wrap = window.localStorage.getItem(storageKey) === 'true';
} catch {
  // Keep editor settings available in memory when local storage is unavailable.
}

export function useEditorWrap() {
  const [value, setValue] = useState(wrap);
  useEffect(() => {
    const changed = () => setValue(wrap);
    window.addEventListener(changeEvent, changed);
    return () => window.removeEventListener(changeEvent, changed);
  }, []);
  function update(value: boolean) {
    wrap = value;
    setValue(value);
    try {
      window.localStorage.setItem(storageKey, String(value));
    } catch {
      // Apply the preference even if it cannot be persisted.
    }
    window.dispatchEvent(new Event(changeEvent));
  }
  return [value, update] as const;
}
