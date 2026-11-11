import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Ghostty } from 'ghostty-web';
import { TerminalOsc } from '../src/terminal-osc.ts';
import { TerminalLinks } from '../src/terminal-links.ts';
const encode = (text) => [...new TextEncoder().encode(text)];
const colors = { foreground: '#Ab12Cd', background: '#012345' };
const input =
  'café 日本🙂\x1b[32mgreen\x1b[0m' +
  '\x1b]666;vte.shell.precmd!\x1b\\\x1b]7501;state=working:app=codex\x07' +
  '\x1b]10;?\x07\x1b]11;?\x1b\\' +
  '\x1b]2;title\x07\x1b]8;;https://example.test\x1b\\link\x1b]8;;\x07';
const expected = encode(
  'café 日本🙂\x1b[32mgreen\x1b[0m' +
    '\x1b]2;title\x07\x1b]8;;https://example.test\x1b\\link\x1b]8;;\x07',
);
const bytes = encode(input);
for (let split = 0; split <= bytes.length; split++) {
  const output = [],
    replies = [];
  const adapter = new TerminalOsc(
    (chunk) => output.push(...chunk),
    () => colors,
    (text) => replies.push(text),
  );
  adapter.write(bytes.slice(0, split));
  adapter.write(bytes.slice(split));
  assert.deepEqual(output, expected, `raw bytes at split ${split}`);
  assert.deepEqual(replies, ['\x1b]10;rgb:AbAb/1212/CdCd\x07', '\x1b]11;rgb:0101/2323/4545\x1b\\']);
}
const output = [],
  replies = [];
const adapter = new TerminalOsc(
  (chunk) => output.push(...chunk),
  () => colors,
  (text) => replies.push(text),
);
// A false OSC inside DCS/APC, cancellations, oversized/unknown OSC and unknown
// warnings must not disappear. Feed individual bytes like fragmented PTY reads.
const untouched =
  '\x1bPinside\x1b]10;?\x07\x1b\\' +
  '\x1b_inside\x1b]7501;state=idle\x07\x1b\\' +
  '\x1b]10;?\x18cancelled\x1b]11;?\x1a' +
  '\x1b]999;unknown\x07\x1b]10;#abcdef\x07' +
  '\x1b]7501;' +
  'x'.repeat(20000) +
  '\x1b\\' +
  '\x1b[\x1b[31m\x1b\x1b[0m';
for (const byte of encode(untouched)) adapter.write([byte]);
assert.deepEqual(output, encode(untouched));
assert.deepEqual(replies, []);
colors.background = '#ffffff';
for (const byte of encode('\x1b]11;?\x07')) adapter.write([byte]);
assert.equal(replies.pop(), '\x1b]11;rgb:ffff/ffff/ffff\x07');
console.log(
  'PASS: OSC BEL/ST replies, all chunk splits, current colors, raw Unicode/CSI/OSC preservation, cancellation, string isolation and bounded overflow',
);

const wasm = await readFile('node_modules/ghostty-web/ghostty-vt.wasm');
const ghostty = await Ghostty.load(`data:application/wasm;base64,${wasm.toString('base64')}`);
const engine = ghostty.createTerminal(80, 24);
const titles = [],
  warnings = [];
const links = new TerminalLinks(
  { wasmTerm: engine },
  (chunk) => engine.write(Uint8Array.from(chunk)),
  (title) => titles.push(title),
);
const integrated = new TerminalOsc(
  (chunk) => links.write(chunk),
  () => colors,
  (text) => replies.push(text),
);
const log = console.log;
try {
  console.log = (...args) => {
    warnings.push(args.join(' '));
  };
  for (const byte of bytes) integrated.write([byte]);
  assert.deepEqual(warnings, [], 'Known notifications and answered queries do not warn');
  assert.deepEqual(titles, ['title']);
  assert.equal(engine.getLine(0)[0].codepoint, 'c'.codePointAt(0));
  integrated.write(encode('\x1b]999;unsupported\x07'));
  assert.ok(
    warnings.some((text) => text.includes('invalid OSC command')),
    'Other Ghostty warnings remain visible',
  );
} finally {
  console.log = log;
  engine.free();
}
console.log(
  'PASS: real Ghostty integration retains titles/screen bytes and unknown warnings while known OSC warnings disappear',
);
