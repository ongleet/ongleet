import { SdkError, type PluginSdk } from '../sdk';
export async function availableSendTargets(
  tabs: PluginSdk['tabs'],
  sessionId: number,
  tabId: number,
) {
  const list = await tabs.list({ sessionId });
  const candidates = await Promise.all(
    list.tabs
      .filter((t) => t.sessionId === sessionId && t.isOpen && t.id !== tabId)
      .map(async (t) => ({ ...t, capabilities: await tabs.capabilities(t.id).catch(() => null) })),
  );
  return candidates.filter(
    (t) => t.capabilities?.available && t.capabilities.availableActions.includes('send-text'),
  );
}
export async function sendToTab(
  tabs: PluginSdk['tabs'],
  sessionId: number,
  sourceTabId: number,
  targetTabId: number,
  text: string,
) {
  if (new TextEncoder().encode(text).length > 8191)
    throw new SdkError(
      'invalid_arguments',
      'Selection and question must fit within 8 KiB including Enter. Select less text or shorten the question.',
    );
  if (!(await availableSendTargets(tabs, sessionId, sourceTabId)).some((t) => t.id === targetTabId))
    throw new SdkError('unavailable', 'Recipient unavailable');
  const result = (await tabs.invoke(targetTabId, 'send-text', { text, enter: true })) as {
    acknowledged?: boolean;
  };
  if (result.acknowledged !== true) throw new SdkError('unavailable', 'Write not acknowledged');
}
export type SelectedNote = {
  source: 'Note';
  tabId: number;
  noteId: string;
  path: string;
  location: string;
  text: string;
  fullText: string;
  revision: string;
  unsaved?: boolean;
};
export type SelectedCode =
  | SelectedNote
  | {
      source: 'Files' | 'Diff';
      tabId: number;
      path: string;
      location: string;
      text: string;
      context: string;
      revision: string;
      unsaved?: boolean;
    };
export function selectionMessage(selection: SelectedCode, question: string) {
  if (selection.source === 'Note') return noteMessage(selection, question);
  if (!selection.text.trim()) throw new SdkError('invalid_arguments', 'Select some code first.');
  const header = `${selection.source} tab ${selection.tabId}\nFile: ${selection.path}\nLocation: ${selection.location}\nRevision: ${selection.revision}${selection.unsaved ? ' (unsaved editor buffer)' : ''}\n`;
  const content = `${header}\nSelected text:\n${selection.text}\n${question.trim() ? `\nComment / question:\n${question}\n` : ''}`;
  const encoder = new TextEncoder();
  const remaining = 8191 - encoder.encode(content).length;
  if (remaining < 0)
    throw new SdkError(
      'invalid_arguments',
      'Selection and question must fit within 8 KiB including Enter. Select less text or shorten the question.',
    );
  const label = '\nNearby code context:\n';
  if (!selection.context || remaining < encoder.encode(label).length + 4) return content;
  const budget = remaining - encoder.encode(label).length;
  let excerpt = '';
  let length = 0;
  for (const character of selection.context) {
    const size = encoder.encode(character).length;
    if (length + size > budget) break;
    excerpt += character;
    length += size;
  }
  return content + label + excerpt;
}

function noteMessage(note: SelectedNote, question: string) {
  const args = JSON.stringify({ noteId: note.noteId });
  const header = `Note tab ${note.tabId}\nNote ID: ${note.noteId}\nName: ${note.path}\nLocation: ${note.location}\nNotebook revision at capture: ${note.revision}\n\nRead the current note with: ongleet tab state ${note.tabId} --args '${args}' --json\nTo modify it, use ongleet tab invoke ${note.tabId} set-text --args with JSON containing noteId, the latest notebook revision from state, and text (the full replacement Markdown). Wait until state is not stale; reread before editing and preserve changes outside the requested passage. The excerpt below is context, not the authoritative current state.\n${question.trim() ? `\nComment / question:\n${question}\n` : ''}\nNote text / selected passage:\n`;
  const suffix = '\n[Excerpt truncated; read the note using its ID for the full content.]';
  const encoder = new TextEncoder();
  const budget = 8191 - encoder.encode(header).length;
  if (budget < encoder.encode(suffix).length)
    throw new SdkError(
      'invalid_arguments',
      'Note context and question must fit within 8 KiB. Shorten the question.',
    );
  if (encoder.encode(note.text).length <= budget) return header + note.text;
  let excerpt = '';
  let length = 0;
  for (const character of note.text) {
    const size = encoder.encode(character).length;
    if (length + size > budget - encoder.encode(suffix).length) break;
    excerpt += character;
    length += size;
  }
  return header + excerpt + suffix;
}
