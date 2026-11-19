import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile('src/components/agent-activity.ts', 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const { observeAgent } = await import(
  `data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`
);
const tick = () => new Promise((resolve) => setImmediate(resolve));
let notify;
let disposed = 0;
let reads = 0;
let activity = 'working';
let open = true;
let fail = false;
let releaseRead;
const updates = [];
const sdk = {
  storage: {
    subscribe: async (callback) => {
      notify = callback;
      return () => {
        disposed++;
      };
    },
  },
  tabs: {
    list: async ({ sessionId }) => {
      assert.equal(sessionId, 3);
      reads++;
      if (releaseRead !== undefined)
        return new Promise((resolve) => {
          releaseRead = resolve;
        });
      if (fail) throw new Error('Unavailable');
      return {
        tabs: [{ id: 10, sessionId: 3, name: 'Codex', isOpen: open, codexActivity: activity }],
      };
    },
  },
};
const stop = observeAgent(sdk, 3, 10, (value) => updates.push(value));
await tick();
assert.deepEqual(updates, [{ name: 'Codex', activity: 'working' }]);
const initialReads = reads;
await tick();
assert.equal(reads, initialReads, 'No polling while idle');
for (activity of ['approval', 'question', 'working', 'idle', 'stopped']) {
  notify();
  await tick();
  assert.equal(updates.at(-1).activity, activity);
}
activity = null;
notify();
await tick();
assert.equal(updates.at(-1).activity, 'unknown');
open = false;
notify();
await tick();
assert.equal(updates.at(-1).activity, 'unavailable');
fail = true;
notify();
await tick();
assert.equal(updates.at(-1).activity, 'unknown');
fail = false;
open = true;
activity = 'idle';
notify();
await tick();
assert.equal(updates.at(-1).activity, 'idle', 'Recover after a read failure');

// An old in-flight snapshot must not overwrite a newer event.
releaseRead = null;
notify();
notify();
notify();
const resolveOld = releaseRead;
const previous = updates.length;
resolveOld({
  tabs: [{ id: 10, sessionId: 3, name: 'Codex', isOpen: true, codexActivity: 'working' }],
});
await tick();
assert.equal(updates.length, previous);
const resolveNew = releaseRead;
resolveNew({
  tabs: [{ id: 10, sessionId: 3, name: 'Codex', isOpen: true, codexActivity: 'idle' }],
});
await tick();
assert.equal(updates.at(-1).activity, 'idle');

notify();
const resolveAfterStop = releaseRead;
stop();
const count = updates.length;
resolveAfterStop({ tabs: [] });
await tick();
assert.equal(disposed, 1);
assert.equal(updates.length, count, 'No updates after unmount');

let finishSubscribe;
let lateDisposals = 0;
const lateUpdates = [];
const stopEarly = observeAgent(
  {
    ...sdk,
    storage: {
      subscribe: () =>
        new Promise((resolve) => {
          finishSubscribe = resolve;
        }),
    },
  },
  3,
  10,
  (value) => lateUpdates.push(value),
);
stopEarly();
finishSubscribe(() => {
  lateDisposals++;
});
await tick();
assert.equal(lateDisposals, 1);
assert.deepEqual(lateUpdates, []);
const failedUpdates = [];
observeAgent(
  {
    ...sdk,
    storage: {
      subscribe: async () => {
        throw new Error('Detached');
      },
    },
  },
  3,
  10,
  (value) => failedUpdates.push(value),
);
await tick();
assert.deepEqual(failedUpdates, [{ activity: 'unknown' }]);
console.log(
  'Live agent activity, event races, missing hooks, failures and subscription cleanup passed.',
);
