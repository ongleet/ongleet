import assert from 'node:assert/strict';
import { createEngineLoader } from '../src/engine-loader.ts';

// Reproduce the user-visible failure: the lazy engine request fails while the
// dev server is stopped; starting the server must allow a later Start to retry.
let available = false;
let requests = 0;
const engine = {};
const load = createEngineLoader(async () => {
  requests++;
  if (!available) throw new TypeError('Load failed');
  return engine;
});
const first = load();
assert.equal(load(), first, 'Concurrent tabs must share the pending engine load');
await assert.rejects(first, /Load failed/);
available = true;
const retry = load();
assert.notEqual(retry, first, 'An explicit retry must not reuse the rejected request');
assert.equal(load(), retry);
assert.equal(await retry, engine);
assert.equal(await load(), engine);
assert.equal(requests, 2, 'A successful engine must remain shared across subsequent tabs');
console.log('PASS: failed engine loads can retry, concurrent loads are shared, successful engine is retained');
