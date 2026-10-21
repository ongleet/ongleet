import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Ghostty } from 'ghostty-web';
import { consumeOutput } from '../src/byte-transport.ts';
import { suppressHiddenRendering } from '../src/render-visibility.ts';

const wasm = await readFile(new URL('../node_modules/ghostty-web/ghostty-vt.wasm', import.meta.url));
const ghostty = await Ghostty.load(`data:application/wasm;base64,${wasm.toString('base64')}`);
const first = ghostty.createTerminal(80, 24);
const second = ghostty.createTerminal(80, 24);
const encoder = new TextEncoder();
let visible = true;
const paints = [];
// Only the drawing boundary is replaced; all screen updates use real WASM.
const renderer = { render(buffer, forceAll) { paints.push({ text: buffer.getLine(0).map(cell => cell.codepoint ? String.fromCodePoint(cell.codepoint) : ' ').join('').trimEnd(), forceAll }); } };
suppressHiddenRendering(renderer, () => visible);
try {
  consumeOutput(first, Array.from(encoder.encode('first')));
  renderer.render(first, true);
  visible = false;
  for (const byte of encoder.encode(' café λ')) {
    consumeOutput(first, [byte]);
    renderer.render(first, false);
  }
  consumeOutput(second, Array.from(encoder.encode('independent')));
  assert.equal(paints.length, 1, 'A hidden terminal must not draw even as output arrives');
  assert.equal(paints[0].text, 'first');
  visible = true;
  renderer.render(first, true);
  assert.deepEqual(paints[1], { text: 'first café λ', forceAll: true });
  renderer.render(second, true);
  assert.equal(paints[2].text, 'independent');
  assert.deepEqual(first.getDimensions(), { cols: 80, rows: 24 });
  console.log('PASS: independent Ghostty states, hidden output parsing without painting, full redraw on return, retained dimensions');
} finally { first.free(); second.free(); }
