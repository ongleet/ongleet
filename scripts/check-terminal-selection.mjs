import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Ghostty } from 'ghostty-web';
import { ghosttySelectionText } from '../src/terminal-selection.ts';

const wasm = await readFile(
  new URL('../node_modules/ghostty-web/ghostty-vt.wasm', import.meta.url),
);
const ghostty = await Ghostty.load(`data:application/wasm;base64,${wasm.toString('base64')}`);
const engine = ghostty.createTerminal(30, 3, { scrollbackLimit: 2000 });
let range;
const selected = () =>
  ghosttySelectionText(
    engine,
    { col: range.start.x, absoluteRow: range.start.y },
    { col: range.end.x, absoluteRow: range.end.y },
  );
try {
  engine.write('  日本語 🙂 e\u0301\r\nSECOND row');
  range = { start: { x: 0, y: 0 }, end: { x: 29, y: 1 } };
  assert.equal(selected(), '  日本語 🙂 e\u0301\nSECOND row');
  range = { start: { x: 29, y: 1 }, end: { x: 0, y: 0 } };
  assert.equal(
    selected(),
    '  日本語 🙂 e\u0301\nSECOND row',
    'Reverse selection has the same text',
  );
  range = { start: { x: 3, y: 0 }, end: { x: 3, y: 0 } };
  assert.equal(selected(), '日', 'Starting on a continuation cell retains its complete character');
  range = { start: { x: 4, y: 0 }, end: { x: 6, y: 0 } };
  assert.equal(
    selected(),
    '本語',
    'A selection ending on a wide leading cell retains that character',
  );
  engine.write('\r\nTHIRD e\u0301\r\nFOURTH 日本語');
  assert.equal(engine.getScrollbackLength(), 1);
  range = { start: { x: 0, y: 0 }, end: { x: 29, y: 3 } };
  assert.equal(selected(), '  日本語 🙂 e\u0301\nSECOND row\nTHIRD e\u0301\nFOURTH 日本語');
  console.log(
    'Terminal selection preserves Unicode, wide boundaries, leading spaces and scrollback.',
  );
} finally {
  engine.free();
}
