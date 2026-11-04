import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText).toString('base64')}`;
const types = moduleUrl(await readFile('src/sdk/types.ts', 'utf8'));
const { CommentStore, commentMessage, commentTargets, deliverComment } = await import(
  moduleUrl(
    (await readFile('src/plugins/diff/comments.ts', 'utf8')).replaceAll(
      "'../../sdk'",
      JSON.stringify(types),
    ),
  )
);
const records = new Map();
const storage = {
  getItem: (key) => records.get(key) ?? null,
  setItem: (key, value) => records.set(key, value),
};
const store = new CommentStore(1, 2, storage);
const patch = '@@ -1,3 +1,3 @@\n alpha\n-target\n+target\n omega\n';
const file = { path: 'a.ts', oldPath: 'a.ts', patch };
const ids = store.add(store.anchor('/repo', file, 'new', 2), 'Please explain', 'User');
store.reply(ids.threadId, 'First reply', 'Agent');
store.reply(ids.threadId, 'Second reply', 'User');
assert.deepEqual(
  store.get(ids.threadId).comments.map((c) => c.text),
  ['Please explain', 'First reply', 'Second reply'],
);
assert.deepEqual(new CommentStore(1, 2, storage).threads, store.threads);
assert.equal(new CommentStore(1, 3, storage).threads.length, 0);
assert.throws(() => store.reply('missing', 'reply', 'Agent'), /Unknown thread/);
assert.throws(() => store.add(store.anchor('/repo', file), '日'.repeat(1366), 'Agent'), /4 KiB/);
assert.throws(() => store.anchor('/repo', file, 'old', 99), /Unknown diff line/);
assert.throws(() => store.anchor('/repo', file, undefined, 2), /together/);
store.reconcile('/repo', [
  { ...file, patch: '@@ -8,3 +8,3 @@\n alpha\n-target\n+target\n omega\n' },
]);
assert.equal(store.get(ids.threadId).location.line, 9);
assert.equal(store.get(ids.threadId).orphan, false);
store.reconcile('/repo', [{ ...file, patch: undefined, failed: true }]);
assert.equal(store.get(ids.threadId).orphan, false);
store.reconcile('/repo', [{ ...file, path: 'renamed.ts', oldPath: 'a.ts' }]);
assert.equal(store.get(ids.threadId).location.path, 'renamed.ts');
store.reconcile('/other', [file]);
assert.equal(store.get(ids.threadId).orphan, true);
store.reply(ids.threadId, 'Reply to orphan', 'Agent');
store.reconcile('/repo', [file]);
assert.equal(store.get(ids.threadId).orphan, false);
store.reconcile('/repo', [
  { ...file, patch: patch + '@@ -10,3 +10,3 @@\n alpha\n-target\n+target\n omega\n' },
]);
assert.equal(store.get(ids.threadId).orphan, true);
store.reconcile('/repo', []);
assert.equal(store.get(ids.threadId).orphan, true);
assert.equal(store.page(0, 1).nextCommentOffset, 1);
assert.equal(store.page(1, 1).comments[0].comment.text, 'First reply');
for (let i = 0; i < 6; i++) store.reply(ids.threadId, '日'.repeat(1300), 'Agent');
const message = commentMessage(2, store.get(ids.threadId), ids.commentId);
assert(Buffer.byteLength(message) + 1 <= 8192);
assert(
  message.includes(ids.threadId) &&
    message.includes(ids.commentId) &&
    message.includes('a.ts') &&
    message.includes('Please explain'),
);
assert.throws(() => commentMessage(2, store.get(ids.threadId), 'missing'), /Unknown comment/);
assert(Buffer.byteLength(JSON.stringify(store.page())) < 33000);
const failing = new CommentStore(1, 2, {
  ...storage,
  setItem: () => {
    throw new Error('quota');
  },
});
assert.throws(() => failing.reply(ids.threadId, 'not saved', 'Agent'), /Could not save/);
assert(!failing.get(ids.threadId).comments.some((c) => c.text === 'not saved'));
store.delete(ids.threadId);
assert.equal(new CommentStore(1, 2, storage).threads.length, 0);
console.log(
  'Diff comments persistence, anchors, orphans, replies, bounds and storage failures passed.',
);

const gaps = new CommentStore(3, 4, storage);
const gapFile = { ...file, patch: '@@ -1 +1 @@\n one\n@@ -20 +20 @@\n twenty\n' };
const gapId = gaps.add(gaps.anchor('/repo', gapFile, 'new', 20), 'Hunk boundary', 'User').threadId;
gaps.reconcile('/repo', [gapFile]);
assert.equal(gaps.get(gapId).orphan, false);
const corrupt = new CommentStore(9, 9, { getItem: () => '{"version":1,"threads":[{}]}' });
assert(corrupt.error.includes('Could not load'));

const sendStore = new CommentStore(5, 6, storage);
const sendIds = sendStore.add(sendStore.anchor('/repo', file), 'Send me', 'User');
const sendThread = sendStore.get(sendIds.threadId);
let calls = [];
let failure;
const tabs = {
  list: async () => ({
    tabs: [
      { id: 6, sessionId: 5, isOpen: true, name: 'Diff' },
      { id: 7, sessionId: 5, isOpen: true, name: 'Agent shell' },
      { id: 8, sessionId: 9, isOpen: true, name: 'Other session' },
      { id: 9, sessionId: 5, isOpen: false, name: 'Closed' },
      { id: 10, sessionId: 5, isOpen: true, name: 'Files' },
      { id: 11, sessionId: 5, isOpen: true, name: 'Unavailable' },
    ],
  }),
  capabilities: async (id) => ({
    available: id !== 11,
    availableActions: id === 10 ? [] : ['send-text'],
  }),
  invoke: async (...args) => {
    calls.push(args);
    if (failure) throw failure;
    return { acknowledged: true };
  },
};
assert.deepEqual(
  (await commentTargets(tabs, 5, 6)).map((t) => t.name),
  ['Agent shell'],
);
await deliverComment(tabs, 5, 6, sendThread, sendIds.commentId, 7);
assert.deepEqual(calls, [
  [7, 'send-text', { text: commentMessage(6, sendThread, sendIds.commentId), enter: true }],
]);
await assert.rejects(
  deliverComment(tabs, 5, 6, sendThread, sendIds.commentId, 8),
  /Recipient unavailable/,
);
assert.equal(calls.length, 1);
failure = new Error('Ambiguous timeout; may have executed');
await assert.rejects(deliverComment(tabs, 5, 6, sendThread, sendIds.commentId, 7), /Ambiguous/);
assert.equal(calls.length, 2); // One attempt for the failed delivery, with no retry.
console.log(
  'Send target scope, availability, exact payload and ambiguous failure without retries passed.',
);

const sideFile = { ...file, patch: '@@ -2,2 +3,2 @@\n-old value\n+new value\n tail\n' };
assert.deepEqual(store.anchor('/repo', sideFile, 'old', 2).context, ['old value', 'tail']);
assert.deepEqual(store.anchor('/repo', sideFile, 'new', 3).context, ['new value', 'tail']);
