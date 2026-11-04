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
export type SelectedCode = {
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
