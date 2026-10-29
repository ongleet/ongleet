import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Ghostty, SelectionManager } from 'ghostty-web';
import {
  ghosttySelectionText,
  attachTerminalSelection,
  terminalSelectionText,
} from '../src/terminal-selection.ts';

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
  // Use pinned Ghostty's actual outside-click and canvas handlers. Only DOM
  // event targets and pixel metrics are substitutes; WASM/selection stay real.
  const previousDocument = globalThis.document;
  globalThis.document = new EventTarget();
  const canvas = Object.assign(new EventTarget(), { contains: (target) => target === canvas });
  const listeners = new Set();
  const terminal = {
    wasmTerm: engine,
    cols: 30,
    rows: 3,
    viewportY: 0,
    onScroll(listener) {
      listeners.add(listener);
      return { dispose: () => listeners.delete(listener) };
    },
  };
  const manager = new SelectionManager(
    terminal,
    { getCanvas: () => canvas, getMetrics: () => ({ width: 1, height: 1 }) },
    engine,
    {},
  );
  terminal.selectionManager = manager;
  const select = () => {
    manager.selectionStart = { col: 0, absoluteRow: 1 };
    manager.selectionEnd = { col: 5, absoluteRow: 1 };
  };
  try {
    select();
    document.dispatchEvent(new Event('click'));
    assert.equal(manager.hasSelection(), false, 'Pinned default clears on outside click');
    select();
    const selectedText = terminalSelectionText(terminal);
    const selection = attachTerminalSelection(terminal);
    document.dispatchEvent(new Event('click'));
    assert.equal(manager.hasSelection(), true, 'Navigation must preserve the existing selection');
    assert.equal(terminalSelectionText(terminal), selectedText);
    const down = Object.assign(new Event('mousedown'), { button: 0, offsetX: 2, offsetY: 1 });
    canvas.dispatchEvent(down);
    assert.equal(manager.isSelecting, true, 'Canvas gestures still start a new selection');
    assert.deepEqual(manager.selectionStart, { col: 2, absoluteRow: 2 });
    manager.isSelecting = false;
    select();
    selection.dispose();
    assert.equal(listeners.size, 0);
    document.dispatchEvent(new Event('click'));
    assert.equal(manager.hasSelection(), false, 'Disposal restores the upstream listener');
  } finally {
    manager.dispose();
    globalThis.document = previousDocument;
  }
  console.log(
    'Pinned selection adapter retains navigation selections, permits canvas gestures and restores listeners on cleanup.',
  );
} finally {
  engine.free();
}
