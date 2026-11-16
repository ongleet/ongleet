import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ThreadPort } from './diff-syntax-worker-port.mjs';
import { parseDiff } from 'react-diff-view';
import { SyntaxClient } from '../src/plugins/diff/src/syntax-client.ts';

const patch = (value) => `diff --git a/example.ts b/example.ts
--- a/example.ts
+++ b/example.ts
@@ -1 +1 @@
-const value = "old";
+const value = ${value};
`;
const hunks = (value) => parseDiff(patch(value))[0].hunks;
const text = (nodes) =>
  nodes.map((node) => (node.type === 'text' ? node.value : text(node.children))).join('');

test('real worker colors both sides and preserves Unicode and source text', async () => {
  const client = new SyntaxClient(() => new ThreadPort());
  const request = client.request('example.ts', patch('42 /* café */'), hunks('42 /* café */'));
  const tokens = await request.result;
  assert.equal(text(tokens.old[0]), 'const value = "old";');
  assert.equal(text(tokens.new[0]), 'const value = 42 /* café */;');
  assert.ok(tokens.new[0].some((node) => node.className === 'diff-syntax-number'));
  request.cancel();
});

// Controlled delivery exposes cancellation races without relying on thread timing.
class ControlledPort {
  sent = [];
  stopped = false;
  postMessage(data) {
    this.sent.push(data);
  }
  terminate() {
    this.stopped = true;
  }
  reply(tokens, id = this.sent.at(-1).id) {
    this.onmessage({ data: { id, tokens } });
  }
}
const colored = {
  old: [[{ type: 'text', value: 'before' }]],
  new: [[{ type: 'text', value: 'after' }]],
};
function controlled() {
  const ports = [];
  const client = new SyntaxClient(() => {
    const port = new ControlledPort();
    ports.push(port);
    return port;
  });
  const request = (value, filename = 'example.ts') =>
    client.request(filename, patch(value), hunks(value));
  return { client, ports, request };
}

test('shares identical work, caches completed patches and distinguishes language and content', async () => {
  const { ports, request } = controlled();
  const first = request('42'),
    second = request('42');
  assert.equal(ports[0].sent.length, 1);
  first.cancel();
  assert.equal(await first.result, undefined);
  assert.equal(ports[0].stopped, false);
  ports[0].reply(colored);
  assert.deepEqual(await second.result, colored);
  assert.deepEqual(await request('42').result, colored);
  assert.equal(ports[0].sent.length, 1);
  const changed = request('43'),
    language = request('42', 'example.py');
  assert.equal(ports[0].sent.length, 2);
  ports[0].reply(colored);
  await changed.result;
  assert.equal(ports[0].sent.at(-1).filename, 'example.py');
  language.cancel();
  assert.equal(await language.result, undefined);
});

test('cancels queued and active work; ignores late results from a terminated worker', async () => {
  const { ports, request } = controlled();
  const old = request('1'),
    queued = request('2'),
    current = request('3');
  queued.cancel();
  old.cancel();
  assert.equal(await queued.result, undefined);
  assert.equal(await old.result, undefined);
  assert.equal(ports[0].stopped, true);
  assert.equal(ports[1].sent.length, 1);
  assert.equal(ports[1].sent[0].hunks[0].changes.at(-1).content, 'const value = 3;');
  ports[0].reply({ old: [], new: [] });
  ports[1].reply(colored);
  assert.deepEqual(await current.result, colored);
});

test('worker failures leave active and queued patches plain and allow a later retry', async () => {
  const { ports, request } = controlled();
  const active = request('1'),
    queued = request('2');
  ports[0].onerror(new Error('worker failed'));
  assert.equal(await active.result, undefined);
  assert.equal(await queued.result, undefined);
  const retry = request('1');
  ports[1].reply(colored);
  assert.deepEqual(await retry.result, colored);
});

test('completed cache evicts old patches instead of retaining every opened file', async () => {
  const { ports, request } = controlled();
  for (let i = 0; i < 17; i++) {
    const pending = request(String(i));
    ports[0].reply(colored);
    await pending.result;
  }
  const old = request('0');
  assert.equal(ports[0].sent.length, 18);
  old.cancel();
  assert.equal(await old.result, undefined);
});

test('unavailable workers fall back to plain text without rejecting', async () => {
  const client = new SyntaxClient(() => {
    throw new Error('Worker unavailable');
  });
  const request = client.request('example.ts', patch('42'), hunks('42'));
  assert.equal(await request.result, undefined);
});

test('a stalled worker is terminated and queued requests settle as plain text', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const { ports, request } = controlled();
  const active = request('1'),
    queued = request('2');
  t.mock.timers.tick(10_000);
  assert.equal(ports[0].stopped, true);
  assert.equal(await active.result, undefined);
  assert.equal(await queued.result, undefined);
});
