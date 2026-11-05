import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { mock } from 'node:test';
import ts from 'typescript';
const url = (source) =>
  `data:text/javascript;base64,${Buffer.from(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText).toString('base64')}`;
const types = url(await readFile('src/sdk/types.ts', 'utf8'));
const { sendPtyText } = await import(
  url((await readFile('src/sdk/pty-text.ts', 'utf8')).replace("'./types'", JSON.stringify(types)))
);
mock.timers.enable({ apis: ['setTimeout'] });
try {
  const writes = [];
  let acknowledge;
  const text = 'Review this code:\n日本語🙂\n' + 'line of context\n'.repeat(450);
  const sending = sendPtyText(
    async (bytes) => {
      writes.push(Buffer.from(bytes));
      if (writes.length === 1)
        await new Promise((resolve) => {
          acknowledge = resolve;
        });
    },
    text,
    true,
  );
  mock.timers.tick(1000);
  assert.equal(writes.length, 1, 'Never submit before the text write is acknowledged');
  assert.equal(writes[0].toString(), text);
  acknowledge();
  await Promise.resolve();
  await Promise.resolve();
  mock.timers.tick(499);
  assert.equal(writes.length, 1, 'Enter must stay outside the paste processing window');
  mock.timers.tick(1);
  await sending;
  assert.deepEqual([...writes[1]], [13]);

  const framed = [];
  const maxText = 'x'.repeat(8191);
  const framedSend = sendPtyText(
    async (bytes) => {
      framed.push(Buffer.from(bytes));
    },
    maxText,
    true,
    true,
  );
  // Both framing chunks must be acknowledged before scheduling Enter.
  for (let i = 0; i < 6; i++) await Promise.resolve();
  assert(framed.every((chunk) => chunk.length <= 8192));
  assert.equal(Buffer.concat(framed).toString(), '\x1b[200~' + maxText + '\x1b[201~');
  mock.timers.tick(500);
  await framedSend;
  assert.deepEqual([...framed.at(-1)], [13], 'Submission is outside bracketed paste');

  const raw = [];
  await sendPtyText(
    async (bytes) => {
      raw.push(bytes);
    },
    'hello\n',
    false,
  );
  assert.deepEqual(raw, [[104, 101, 108, 108, 111, 10]]);
  await sendPtyText(
    async (bytes) => {
      raw.push(bytes);
    },
    '',
    true,
  );
  assert.deepEqual(raw[1], [13], 'Empty submission sends exactly one Enter');
  let attempts = 0;
  await assert.rejects(
    sendPtyText(
      async () => {
        attempts++;
        throw new Error('write failed');
      },
      'text',
      true,
    ),
    /write failed/,
  );
  mock.timers.tick(1000);
  assert.equal(attempts, 1, 'Failed text writes never submit or retry');
  await assert.rejects(
    sendPtyText(
      async () => {
        throw new Error('must not write');
      },
      'x'.repeat(8192),
      true,
    ),
    /8 KiB/,
  );
  console.log(
    'PTY text: multiline UTF-8, acknowledgement, delayed Enter, literal input, failures and bounds passed.',
  );
} finally {
  mock.timers.reset();
}
