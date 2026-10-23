import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Ghostty } from 'ghostty-web';
import { consumeOutput, initializeTerminal, SCROLLBACK_LINES } from '../src/byte-transport.ts';

const wasm = await readFile(
  new URL('../node_modules/ghostty-web/ghostty-vt.wasm', import.meta.url),
);
const engine = await Ghostty.load(`data:application/wasm;base64,${wasm.toString('base64')}`);
const encoder = new TextEncoder();
const config = { scrollbackLimit: SCROLLBACK_LINES };
const sibling = engine.createTerminal(80, 24, config);
initializeTerminal(sibling);
consumeOutput(sibling, Array.from(encoder.encode('independent sibling café 日本語 🙂')));
const text = (terminal, row) =>
  terminal
    .getLine(row)
    .filter((cell) => cell.width !== 0)
    .map((cell) => (cell.codepoint ? String.fromCodePoint(cell.codepoint) : ' '))
    .join('')
    .trimEnd();
try {
  for (let iteration = 0; iteration < 30; iteration++) {
    const previous = engine.createTerminal(80, 24, config);
    try {
      consumeOutput(
        previous,
        Array.from(encoder.encode(`CLOSED-TAB-${iteration}\r\n`.repeat(800))),
      );
      if (iteration % 2) previous.resize(100, 32);
      consumeOutput(
        previous,
        Array.from(encoder.encode('\x1b[?1049h\x1b[?2004h\x1b[?1000hALT CLOSED')),
      );
    } finally {
      previous.free();
    }
    const fresh = engine.createTerminal(80, 24, config);
    try {
      const [cols, rows] = [
        [80, 24],
        [85, 37],
        [94, 44],
      ][iteration % 3];
      fresh.resize(cols, rows);
      initializeTerminal(fresh);
      for (let row = 0; row < rows; row++) {
        assert.equal(
          text(fresh, row),
          '',
          `Closed tab leaked content into new terminal ${iteration}, row ${row}`,
        );
      }
      assert.equal(fresh.getScrollbackLength(), 0);
      assert.equal(fresh.isAlternateScreen(), false);
      assert.equal(fresh.hasBracketedPaste(), false);
      assert.equal(fresh.hasMouseTracking(), false);
      consumeOutput(fresh, Array.from(encoder.encode(`fresh ${iteration}: café 日本語 🙂`)));
      assert.equal(text(fresh, 0), `fresh ${iteration}: café 日本語 🙂`);
      assert.equal(text(sibling, 0), 'independent sibling café 日本語 🙂');
    } finally {
      fresh.free();
    }
  }
  console.log(
    'PASS: 30 create/close cycles with shared WASM, resize, alternate screens and flood; new screens/modes are clean and sibling state is unchanged',
  );
} finally {
  sibling.free();
}
