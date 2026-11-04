import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const url = (source) =>
  `data:text/javascript;base64,${Buffer.from(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText).toString('base64')}`;
const types = url(await readFile('src/sdk/types.ts', 'utf8'));
const { availableSendTargets, selectionMessage, sendToTab } = await import(
  url(
    (await readFile('src/components/tab-send.ts', 'utf8')).replaceAll(
      "'../sdk'",
      JSON.stringify(types),
    ),
  )
);
const selection = {
  source: 'Files',
  tabId: 2,
  path: '/repo/a.ts',
  location: 'lines 2:1–2:6',
  text: 'hello',
  context: 'before\nhello\nafter',
  revision: 'revision-1',
  unsaved: true,
};
assert.equal(
  selectionMessage(selection, 'Why?'),
  'Files tab 2\nFile: /repo/a.ts\nLocation: lines 2:1–2:6\nRevision: revision-1 (unsaved editor buffer)\n\nSelected text:\nhello\n\nComment / question:\nWhy?\n\nNearby code context:\nbefore\nhello\nafter',
);
assert(!selectionMessage(selection, '').includes('Comment / question:'));
assert.throws(() => selectionMessage({ ...selection, text: ' ' }, ''), /Select/);
assert.throws(() => selectionMessage({ ...selection, text: '日'.repeat(3000) }, ''), /8 KiB/);
assert.throws(() => selectionMessage(selection, 'x'.repeat(8192)), /8 KiB/);
const bounded = selectionMessage({ ...selection, context: '日🙂'.repeat(4000) }, 'A question');
assert(Buffer.byteLength(bounded) + 1 <= 8192);
assert(bounded.includes('hello') && bounded.includes('A question') && !bounded.includes('\uFFFD'));
let available = true,
  failure = false,
  calls = [];
const tabs = {
  list: async () => ({
    tabs: [
      { id: 1, sessionId: 3, isOpen: true, name: 'Agent' },
      { id: 2, sessionId: 3, isOpen: true, name: 'Files' },
      { id: 4, sessionId: 5, isOpen: true, name: 'Other session' },
      { id: 5, sessionId: 3, isOpen: false, name: 'Closed' },
      { id: 6, sessionId: 3, isOpen: true, name: 'Diff' },
      { id: 7, sessionId: 3, isOpen: true, name: 'Disconnected' },
    ],
  }),
  capabilities: async (id) => {
    if (id === 7) throw new Error('Detached');
    return { available, availableActions: id === 6 ? [] : ['send-text'] };
  },
  invoke: async (...args) => {
    calls.push(args);
    if (failure) throw new Error('Timed out; may have executed');
    return { acknowledged: true };
  },
};
assert.deepEqual(
  (await availableSendTargets(tabs, 3, 2)).map((tab) => tab.name),
  ['Agent'],
);
await sendToTab(tabs, 3, 2, 1, 'selected text');
assert.deepEqual(calls, [[1, 'send-text', { text: 'selected text', enter: true }]]);
available = false;
await assert.rejects(sendToTab(tabs, 3, 2, 1, 'selection'), /unavailable/);
assert.equal(calls.length, 1);
available = true;
failure = true;
await assert.rejects(sendToTab(tabs, 3, 2, 1, 'selection'), /Timed out/);
assert.equal(calls.length, 2);
await assert.rejects(sendToTab(tabs, 3, 2, 4, 'selection'), /unavailable/);
await assert.rejects(sendToTab(tabs, 3, 2, 1, 'x'.repeat(8192)), /8 KiB/);
assert.equal(calls.length, 2);
console.log(
  'Selection payload context, unsaved provenance, UTF-8 bounds, target scope and sends without retries passed.',
);
