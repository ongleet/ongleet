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
const note = {
  source: 'Note',
  tabId: 42,
  noteId: 'stable-note-id',
  path: 'Plan',
  location: 'Entire note',
  text: '日🙂'.repeat(6000),
  fullText: '日🙂'.repeat(6000),
  revision: '8',
};
const notePayload = selectionMessage(note, 'Summarize this note.');
assert(Buffer.byteLength(notePayload) <= 8191);
assert(!notePayload.includes('\uFFFD'));
assert(notePayload.includes('Note ID: stable-note-id'));
assert(notePayload.includes('ongleet tab state 42 --args \'{"noteId":"stable-note-id"}\' --json'));
assert(notePayload.includes('ongleet tab invoke 42 set-text'));
assert(notePayload.includes('latest notebook revision'));
assert(notePayload.includes('Summarize this note.') && notePayload.includes('Excerpt truncated'));
const passage = selectionMessage(
  { ...note, text: 'Selected passage', location: 'characters 3–19' },
  'Rephrase',
);
assert(passage.includes('characters 3–19') && passage.includes('Selected passage'));
assert(!passage.includes('Excerpt truncated'));
assert(
  selectionMessage({ ...note, text: '', fullText: '' }, 'Write a plan').includes('Write a plan'),
);
assert.throws(() => selectionMessage(note, 'x'.repeat(8192)), /8 KiB/);
let available = true,
  failure = false,
  calls = [];
const tabs = {
  list: async () => ({
    tabs: [
      { id: 1, sessionId: 3, isOpen: true, name: 'Agent', agentReceiver: true },
      { id: 2, sessionId: 3, isOpen: true, name: 'Files' },
      { id: 4, sessionId: 5, isOpen: true, name: 'Other session' },
      { id: 5, sessionId: 3, isOpen: false, name: 'Closed' },
      { id: 8, sessionId: 3, isOpen: true, name: 'Ordinary shell' },
      { id: 6, sessionId: 3, isOpen: true, name: 'Diff' },
      { id: 7, sessionId: 3, isOpen: true, name: 'Disconnected', agentReceiver: true },
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

await assert.rejects(sendToTab(tabs, 3, 2, 8, 'Never execute in a shell'), /Recipient unavailable/);
const preferences = new Map();
globalThis.localStorage = {
  getItem: (key) => preferences.get(key) ?? null,
  setItem: (key, value) => preferences.set(key, value),
};
const { preferredAgent, rememberAgent } = await import(
  url(await readFile('src/components/agent-recipient.ts', 'utf8'))
);
const agents = [
  { id: 10, name: 'Codex' },
  { id: 20, name: 'Other agent' },
];
assert.equal(preferredAgent(agents, 3), undefined);
assert.equal(preferredAgent([agents[0]], 3), 10);
rememberAgent(3, 20, '2.thread-a');
assert.equal(preferredAgent(agents, 3), 20);
assert.equal(preferredAgent(agents, 4), undefined);
rememberAgent(3, 10);
assert.equal(preferredAgent(agents, 3), 10);
assert.equal(preferredAgent(agents, 3, '2.thread-a'), 20);
assert.equal(preferredAgent(agents, 3, '2.thread-b'), 10);
assert.equal(
  preferredAgent(
    [
      { id: 30, name: 'New' },
      { id: 40, name: 'Another' },
    ],
    3,
  ),
  undefined,
);
assert.equal(preferredAgent([], 3), undefined);
globalThis.localStorage = {
  getItem() {
    throw new Error('Unavailable');
  },
  setItem() {
    throw new Error('Full');
  },
};
assert.equal(preferredAgent(agents, 3), undefined);
assert.doesNotThrow(() => rememberAgent(3, 10));
console.log('Agent filtering and persistent recipient selection passed.');

let discoveries = 0;
let finishDiscovery;
const concurrentTabs = {
  ...tabs,
  list: () => {
    discoveries++;
    return new Promise((resolve) => {
      finishDiscovery = resolve;
    });
  },
};
const firstDiscovery = availableSendTargets(concurrentTabs, 3, 2);
const secondDiscovery = availableSendTargets(concurrentTabs, 3, 2);
assert.equal(discoveries, 1);
finishDiscovery({
  tabs: [{ id: 1, sessionId: 3, isOpen: true, agentReceiver: true, name: 'Agent' }],
});
assert.equal((await firstDiscovery)[0].id, 1);
assert.equal((await secondDiscovery)[0].id, 1);
const refreshedDiscovery = availableSendTargets(concurrentTabs, 3, 2);
assert.equal(discoveries, 2);
finishDiscovery({ tabs: [] });
assert.deepEqual(await refreshedDiscovery, []);
console.log('Concurrent agent discovery shares requests without retaining stale recipients.');

// The last metadata read guards automatic sends after capabilities discovery.
let idleChecks = 0;
let guardedWrites = 0;
const guarded = {
  ...tabs,
  list: async () => {
    idleChecks++;
    return {
      tabs: [
        {
          id: 1,
          sessionId: 3,
          isOpen: true,
          agentReceiver: true,
          name: 'Codex',
          codexActivity: idleChecks % 2 ? 'idle' : 'working',
        },
      ],
    };
  },
  invoke: async () => {
    guardedWrites++;
    return { acknowledged: true };
  },
};
assert.equal(await sendToTab(guarded, 3, 2, 1, 'defer until free', true), 'working');
assert.equal(guardedWrites, 0);
guarded.list = async () => ({
  tabs: [
    {
      id: 1,
      sessionId: 3,
      isOpen: true,
      agentReceiver: true,
      name: 'Codex',
      codexActivity: 'idle',
    },
  ],
});
assert.equal(await sendToTab(guarded, 3, 2, 1, 'ready', true), undefined);
assert.equal(guardedWrites, 1);
console.log('Automatic sends recheck activity immediately before the write.');
