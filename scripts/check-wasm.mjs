import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Ghostty } from 'ghostty-web';
import { consumeOutput, SCROLLBACK_LINES, MAX_OUTPUT_CHUNK } from '../src/byte-transport.ts';

// Exercise the production byte adapter against the actual pinned WASM, not a
// simulated terminal. This does not certify rendering or WebKitGTK input.
// The published browser bundle stubs filesystem imports. Use fetch's data URL
// path with the exact installed bytes; the app fetches Vite's emitted asset.
const wasm = await readFile(new URL('../node_modules/ghostty-web/ghostty-vt.wasm', import.meta.url));
const ghostty = await Ghostty.load(`data:application/wasm;base64,${wasm.toString('base64')}`);
const terminal = ghostty.createTerminal(80, 24, { scrollbackLimit: SCROLLBACK_LINES });
const encoder = new TextEncoder();
function write(text, split = false) {
  const bytes = encoder.encode(text);
  if (split) for (const byte of bytes) consumeOutput(terminal, [byte]);
  else for (let start = 0; start < bytes.length; start += MAX_OUTPUT_CHUNK) {
    consumeOutput(terminal, Array.from(bytes.subarray(start, start + MAX_OUTPUT_CHUNK)));
  }
}
function line(row) {
  return (terminal.getLine(row) ?? []).filter(cell => cell.width !== 0)
    .map(cell => cell.codepoint ? String.fromCodePoint(cell.codepoint) : ' ').join('').trimEnd();
}

try {
  write('split: café λ 🙂', true);
  assert.equal(line(0), 'split: café λ 🙂');
  write('\x1b[?1049hALT', true);
  assert.equal(terminal.isAlternateScreen(), true);
  assert.ok(line(0).includes('ALT'));
  write('\x1b[?1049l', true);
  assert.equal(terminal.isAlternateScreen(), false);
  assert.equal(line(0), 'split: café λ 🙂');
  write('\x1b[?1000h\x1b[?1006h\x1b[?2004h', true);
  assert.equal(terminal.hasMouseTracking(), true);
  assert.equal(terminal.hasBracketedPaste(), true);
  write('\x1b[6n', true);
  assert.ok(terminal.readResponse()?.endsWith('R'));
  assert.throws(() => consumeOutput(terminal, new Array(MAX_OUTPUT_CHUNK + 1).fill(65)), /limit/);
  for (let batch = 0; batch < 100; batch++) write('bounded output\r\n'.repeat(1000));
  assert.ok(terminal.getScrollbackLength() <= SCROLLBACK_LINES, `Scrollback grew to ${terminal.getScrollbackLength()}`);
  terminal.resize(100, 32);
  assert.deepEqual(terminal.getDimensions(), { cols: 100, rows: 32 });
  console.log('PASS: actual Ghostty WASM + byte adapter: split UTF-8/VT, alternate screen restoration, modes, DSR response, chunk rejection, 100,000 output lines, bounded scrollback, resize');
} finally {
  terminal.free();
}
