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

// Reload rebuilds the existing engine from a bounded native byte/resize journal.
// Check the screen and modes, including split UTF-8 and alternate-screen state.
const replayEngine = await Ghostty.load(`data:application/wasm;base64,${wasm.toString('base64')}`);
const live = replayEngine.createTerminal(80, 24, { scrollbackLimit: SCROLLBACK_LINES });
const reloadedEngine = await Ghostty.load(`data:application/wasm;base64,${wasm.toString('base64')}`);
const reloaded = reloadedEngine.createTerminal(80, 24, { scrollbackLimit: SCROLLBACK_LINES });
const journal = [];
function recordText(text) {
  for (const byte of encoder.encode(text)) {
    journal.push({ bytes: [byte] });
    consumeOutput(live, [byte]);
  }
}
function assertSameScreen() {
  assert.deepEqual(reloaded.getDimensions(), live.getDimensions());
  assert.equal(reloaded.isAlternateScreen(), live.isAlternateScreen());
  assert.equal(reloaded.hasBracketedPaste(), live.hasBracketedPaste());
  assert.equal(reloaded.hasMouseTracking(), live.hasMouseTracking());
  const { rows } = live.getDimensions();
  for (let row = 0; row < rows; row++) {
    const actual = JSON.stringify(reloaded.getLine(row));
    const expected = JSON.stringify(live.getLine(row));
    assert.equal(actual === expected, true, `Replay screen mismatch at row ${row}`);
  }
}
try {
  recordText('before reload: café 日本語 🙂\r\n');
  recordText('\x1b[?1049h\x1b[?2004h\x1b[?1000h\x1b[?1006hCodex-like alternate screen');
  live.resize(100, 32); journal.push({ cols: 100, rows: 32 });
  recordText('\r\nsecond line λ é\x1b[6n');
  for (const event of journal) {
    if (event.bytes) consumeOutput(reloaded, event.bytes);
    else reloaded.resize(event.cols, event.rows);
  }
  assertSameScreen();
  const exitAlternate = Array.from(encoder.encode('\x1b[?1049l'));
  consumeOutput(live, exitAlternate); consumeOutput(reloaded, exitAlternate);
  assertSameScreen();
  console.log('PASS: reload replay preserves real Ghostty screen, split Unicode, alternate/normal buffers, modes and dimensions');
} finally { live.free(); reloaded.free(); }
