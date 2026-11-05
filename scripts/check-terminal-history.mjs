import assert from 'node:assert/strict';
import { mock } from 'node:test';
import { TerminalHistory } from '../src/terminal-history.ts';

mock.timers.enable({ apis: ['setTimeout'] });
const settle = async () => {
  for (let n = 0; n < 12; n++) await Promise.resolve();
};
let text = 'first';
let scans = 0;
const writes = [];
const errors = [];
let block;
let fail = false;
const history = new TerminalHistory(
  () => {
    scans++;
    return text;
  },
  async (value) => {
    writes.push(value);
    if (block) await block;
    if (fail) throw new Error('disk unavailable');
  },
  (error) => errors.push(error),
);
try {
  // A continuously producing terminal must not postpone persistence forever.
  for (let n = 0; n < 49; n++) {
    history.changed();
    mock.timers.tick(100);
    await settle();
  }
  assert.equal(scans, 0);
  mock.timers.tick(100);
  await settle();
  assert.deepEqual(writes, ['first']);
  mock.timers.tick(60000);
  await settle();
  await history.flush();
  assert.equal(scans, 1, 'Clean terminals must not scan even during lifecycle flushes');

  history.changed(); // Cursor/control updates may leave text unchanged.
  mock.timers.tick(5000);
  await settle();
  assert.equal(scans, 2);
  assert.deepEqual(writes, ['first'], 'Unchanged snapshots must skip backend persistence');

  text = 'lifecycle';
  history.changed();
  await history.flush();
  assert.deepEqual(writes, ['first', 'lifecycle'], 'Lifecycle flush bypasses throttle');

  let release;
  block = new Promise((resolve) => {
    release = resolve;
  });
  text = 'in flight';
  history.changed();
  const first = history.flush();
  await settle();
  text = 'arrived during save';
  history.changed();
  const second = history.flush();
  release();
  await Promise.all([first, second]);
  block = undefined;
  assert.deepEqual(writes.slice(-2), ['in flight', 'arrived during save']);
  const before = scans;
  await Promise.all([history.flush(), history.flush()]);
  assert.equal(scans, before, 'Concurrent flushes must not repeat clean scans');

  block = new Promise((resolve) => {
    release = resolve;
  });
  text = 'periodic in flight';
  history.changed();
  mock.timers.tick(5000);
  await settle();
  text = 'periodic trailing output';
  history.changed();
  release();
  await settle();
  block = undefined;
  mock.timers.tick(4999);
  await settle();
  assert.equal(writes.at(-1), 'periodic in flight');
  mock.timers.tick(1);
  await settle();
  assert.equal(
    writes.at(-1),
    'periodic trailing output',
    'Output during a save must schedule another snapshot',
  );

  text = 'retry';
  fail = true;
  history.changed();
  await assert.rejects(history.flush(), /disk unavailable/);
  fail = false;
  await history.flush();
  assert.deepEqual(writes.slice(-2), ['retry', 'retry'], 'Failed saves remain dirty');

  text = 'discarded emulator';
  history.changed();
  history.dispose();
  mock.timers.tick(60000);
  await settle();
  await history.flush();
  assert.equal(writes.at(-1), 'retry', 'Disposed emulators must not save');
  assert.deepEqual(errors, []);
  console.log(
    'PASS: throttled history, clean/unchanged skipping, lifecycle flush, in-flight output, retry and disposal',
  );
} finally {
  history.dispose();
  mock.timers.reset();
}
