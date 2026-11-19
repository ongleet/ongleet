import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const compiled = ts.transpileModule(await readFile('src/components/agent-queue.ts', 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const { AgentQueues } = await import(
  `data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`
);
const tick = () => new Promise((resolve) => setImmediate(resolve));
const storageKey = 'ongleet.agent-queues.v1';
function fixture(saved) {
  let raw = saved ?? null,
    fail = false,
    sequence = 0;
  const storage = {
    getItem: () => raw,
    setItem: (_key, value) => {
      if (fail) throw new Error('Disk full');
      raw = value;
    },
  };
  const queues = new AgentQueues(storage, () => `message-${++sequence}`);
  const notifications = new Map(),
    calls = [],
    acknowledgements = [];
  const port = {
    observe: (session, target, changed) => {
      notifications.set(`${session}.${target}`, changed);
      return () => notifications.delete(`${session}.${target}`);
    },
    send: (session, target, item) => {
      assert.equal(
        JSON.parse(raw)
          .find((q) => q.targetId === target)
          .items.find((m) => m.id === item.id).state,
        'sending',
        'Persist before external side effect',
      );
      return new Promise((resolve, reject) =>
        calls.push({ session, target, text: item.text, resolve, reject }),
      );
    },
    acknowledged: (session, target, item) => acknowledgements.push({ session, target, item }),
  };
  const detach = queues.attach(1, 3, port);
  const add = (text, target = 10, sourceId = 1) =>
    queues.enqueue(3, target, { sourceId, text, label: 'Files: a.ts' });
  const state = (target = 10) => queues.get(3, target);
  const activity = (value, target = 10) => notifications.get(`3.${target}`)(value);
  return {
    queues,
    storage,
    port,
    add,
    state,
    activity,
    calls,
    acknowledgements,
    detach,
    raw: () => raw,
    fail: (value) => {
      fail = value;
    },
  };
}

// One FIFO across source tabs, and one acknowledged send per work/idle cycle.
const f = fixture();
f.queues.ensure(3, 10);
f.activity('working');
const first = f.add('first', 10, 1);
const second = f.add('second', 10, 2);
f.add('third', 10, 1);
assert.equal(f.calls.length, 0);
f.activity('question');
f.activity('approval');
assert.equal(f.calls.length, 0);
f.activity('idle');
assert.deepEqual(
  f.calls.map((call) => call.text),
  ['first'],
);
assert.throws(() => f.queues.edit(3, 10, first, 'changed'), /cannot be edited/);
assert.throws(() => f.queues.remove(3, 10, first), /cannot be removed/);
f.activity('idle');
f.activity('idle');
f.calls[0].resolve();
await tick();
assert.equal(f.calls.length, 1, 'Repeated idle must not drain the queue');
assert.equal(f.state().phase, 'awaiting-work');
f.activity('working');
f.activity('approval');
assert.equal(f.calls.length, 1);
f.activity('idle');
assert.equal(f.calls[1].text, 'second');
// A quick turn can complete before the write acknowledgement.
f.activity('working');
f.activity('idle');
assert.equal(f.calls.length, 2);
f.calls[1].resolve();
await tick();
assert.equal(f.calls[2].text, 'third');
f.calls[2].resolve();
await tick();
assert.equal(f.acknowledgements.length, 3);
assert.equal(f.state().items.length, 0);
assert.equal(JSON.parse(f.raw()).length, 0, 'Acknowledged messages do not persist as pending');

// Editing and removal change the captured payload, without changing FIFO order.
const e = fixture();
const edit = e.add('old');
const cancel = e.add('cancel');
e.queues.edit(3, 10, edit, 'edited');
e.queues.remove(3, 10, cancel);
e.activity('idle');
assert.equal(e.calls[0].text, 'edited');
e.calls[0].resolve();
await tick();

// Unknown/stopped/closed agents pause until explicit resume; waits never send.
for (const unavailable of ['unknown', 'stopped', 'unavailable']) {
  const p = fixture();
  p.add('pending');
  p.activity(unavailable);
  p.activity('idle');
  assert.equal(p.calls.length, 0);
  assert.equal(p.state().paused, 'unavailable');
  p.queues.resume(3, 10);
  assert.equal(p.calls.length, 1);
  p.calls[0].resolve();
  await tick();
}

// Different recipients are independent; manual send bypasses work, never an active write.
const m = fixture();
m.add('busy');
m.activity('working');
m.add('other', 20);
m.activity('idle', 20);
assert.equal(m.calls[0].target, 20);
const busy = m.state().items[0].id;
const forced = m.queues.sendNow(3, 10, busy);
assert.equal(m.calls[1].text, 'busy');
await m.queues.sendNow(3, 10, busy);
assert.equal(m.calls.length, 2);
m.calls.forEach((call) => call.resolve());
await forced;
await tick();

// Ambiguous sends retain an uncertain marker, block the FIFO, and never auto-retry.
const a = fixture();
const uncertain = a.add('maybe');
a.add('next');
a.activity('idle');
a.calls[0].reject(new Error('Timed out after write'));
await tick();
assert.equal(a.state().items[0].state, 'uncertain');
assert.equal(a.acknowledgements.length, 0);
a.activity('working');
a.activity('idle');
assert.equal(a.calls.length, 1);
assert.throws(() => a.queues.resume(3, 10), /uncertain/);
assert.throws(() => a.queues.sendNow(3, 10, a.state().items[1].id), /uncertain/);
a.queues.remove(3, 10, uncertain);
a.queues.resume(3, 10);
assert.equal(a.calls[1].text, 'next');
a.calls[1].resolve();
await tick();

// Restored messages remain paused, including a write interrupted before acknowledgement.
const before = fixture();
before.add('saved');
before.activity('working');
const restored = fixture(before.raw());
restored.queues.ensure(3, 10);
restored.activity('idle');
assert.equal(restored.calls.length, 0);
assert.equal(restored.state().paused, 'restored');
restored.queues.resume(3, 10);
assert.equal(restored.calls[0].text, 'saved');
const crash = fixture(restored.raw());
crash.queues.ensure(3, 10);
crash.activity('idle');
assert.equal(crash.state().items[0].state, 'uncertain');
assert.equal(crash.calls.length, 0);
assert.throws(() => crash.queues.resume(3, 10), /uncertain/);
restored.calls[0].resolve();
await tick();

// Storage failure cannot submit an unpersisted message or silently lose an edit.
const s = fixture();
const keep = s.add('keep');
s.fail(true);
assert.throws(() => s.add('lost'), /Disk full/);
assert.equal(s.state().items.length, 1);
assert.throws(() => s.queues.edit(3, 10, keep, 'lost edit'), /Disk full/);
assert.equal(s.state().items[0].text, 'keep');
assert.throws(() => s.queues.remove(3, 10, keep), /Disk full/);
s.activity('idle');
assert.equal(s.calls.length, 0);
assert.equal(s.state().paused, 'storage');
s.fail(false);
s.queues.resume(3, 10);
assert.equal(s.calls.length, 1, 'Resume after a failed pre-write save can send safely');
s.calls[0].resolve();
await tick();

// Detaching a sender never replays an in-flight write; another same-session source can observe.
const d = fixture();
d.add('once');
d.activity('idle');
d.detach();
d.queues.attach(2, 3, d.port);
d.activity('idle');
assert.equal(d.calls.length, 1);
d.calls[0].resolve();
await tick();
assert.equal(d.state().items.length, 0);

// Bound storage and refuse corrupt saved data rather than overwriting it.
const limit = fixture();
for (let i = 0; i < 20; i++) limit.add(`message ${i}`);
assert.throws(() => limit.add('overflow'), /full/);
assert.throws(() => limit.add('日'.repeat(2800), 20), /8 KiB/);
const corrupt = fixture('{bad json');
assert.throws(() => corrupt.add('must not overwrite'), /unavailable/);
assert.equal(corrupt.raw(), '{bad json');

// Shutdown pauses pending work before waiting for the current acknowledgement.
const q = fixture();
q.add('in flight');
q.add('later');
q.activity('idle');
const closing = q.queues.shutdown();
q.activity('working');
q.activity('idle');
q.calls[0].resolve();
await closing;
await tick();
assert.equal(q.calls.length, 1);
assert.equal(q.state().items[0].text, 'later');
assert.equal(q.state().paused, 'manual');

// A fresh preflight can defer a send if another turn started after the idle event.
const race = fixture();
race.detach();
race.queues.attach(2, 3, { ...race.port, send: async () => 'working' });
race.add('deferred');
race.activity('idle');
await tick();
assert.equal(race.state().items[0].state, 'pending');
assert.equal(race.state().activity, 'working');
assert.equal(race.acknowledgements.length, 0);

// Explicit pause and scoped subscriptions must not affect another recipient/session.
const scoped = fixture();
scoped.add('paused');
scoped.add('independent', 20);
let primaryUpdates = 0;
const stopPrimary = scoped.queues.subscribeTarget(3, 10, () => {
  primaryUpdates++;
});
scoped.queues.pause(3, 10);
const pausedUpdates = primaryUpdates;
scoped.activity('idle', 20);
assert.equal(primaryUpdates, pausedUpdates);
assert.equal(scoped.calls[0].target, 20);
scoped.activity('idle');
assert.equal(scoped.calls.length, 1);
scoped.queues.resume(3, 10);
assert.equal(scoped.calls[1].target, 10);
scoped.calls.forEach((call) => call.resolve());
await tick();
stopPrimary();
scoped.queues.enqueue(4, 10, { sourceId: 1, text: 'other session', label: 'Note' });
assert.equal(scoped.queues.get(4, 10).items.length, 1);
assert.equal(scoped.calls.length, 2, 'A source from another session cannot deliver');

const globalLimit = fixture();
for (let i = 0; i < 64; i++) globalLimit.add(`message ${i}`, 10 + Math.floor(i / 20));
assert.throws(() => globalLimit.add('overflow', 30), /full/);

// Corrupt duplicate IDs must not make an acknowledgement remove two messages.
const validSaved = JSON.parse(before.raw());
validSaved[0].items.push({ ...validSaved[0].items[0] });
const duplicate = fixture(JSON.stringify(validSaved));
assert.throws(() => duplicate.add('new'), /unavailable/);
console.log(
  'Agent FIFO, work/idle gates, edits, forced sends, uncertainty, recovery, storage bounds and ownership passed.',
);

const previouslyUnknown = fixture();
previouslyUnknown.queues.ensure(3, 10);
previouslyUnknown.activity('unknown');
previouslyUnknown.add('must pause');
previouslyUnknown.activity('idle');
assert.equal(previouslyUnknown.calls.length, 0);
assert.equal(previouslyUnknown.state().paused, 'unavailable');

const noOp = fixture();
noOp.queues.ensure(3, 10);
noOp.activity('working');
let noOpRenders = 0;
const stopNoOp = noOp.queues.subscribeTarget(3, 10, () => {
  noOpRenders++;
});
noOp.activity('working');
assert.equal(noOpRenders, 0, 'Unchanged activity must not rerender all discussions');
noOp.activity('idle');
assert.equal(noOpRenders, 1);
stopNoOp();
