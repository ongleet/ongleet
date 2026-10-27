import type { TextDocument } from '../../sdk';

type Draft = { version: 1; baseline: TextDocument; text: string };

function key(id: number, path: string) {
  return `ongleet.editor.draft:${JSON.stringify([id, path])}`;
}

export function readDraft(id: number, path: string): Draft | undefined {
  const raw = window.localStorage.getItem(key(id, path));
  if (raw === null) return;
  const draft = JSON.parse(raw) as Draft;
  const base = draft?.baseline;
  if (
    draft?.version !== 1 ||
    typeof draft.text !== 'string' ||
    base?.path !== path ||
    typeof base.text !== 'string' ||
    typeof base.revision !== 'string' ||
    typeof base.bom !== 'boolean' ||
    !['LF', 'CRLF'].includes(base.lineEnding)
  ) {
    throw new Error('Invalid local editor draft.');
  }
  return draft;
}

export function storeDraft(id: number, baseline: TextDocument, text: string) {
  if (text === baseline.text) {
    window.localStorage.removeItem(key(id, baseline.path));
  } else {
    const draft: Draft = { version: 1, baseline, text };
    window.localStorage.setItem(key(id, baseline.path), JSON.stringify(draft));
  }
}

export function removeDraft(id: number, path: string) {
  window.localStorage.removeItem(key(id, path));
}
