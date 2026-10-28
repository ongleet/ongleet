import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import { CanvasRenderer, Ghostty } from 'ghostty-web';
import { EditorState, Compartment } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { history, undo, redo } from '@codemirror/commands';
import { prepareTerminalRendering } from '../src/terminal-rendering.ts';
const exports = {};
vm.runInNewContext(
  ts.transpileModule(readFileSync('src/theme-presets.ts', 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS },
  }).outputText,
  { exports },
);
const { themePresets, ansiKeys } = exports;
const syntaxExports = {};
const dependencies = {};
for (const name of [
  '@codemirror/language',
  '@codemirror/language-data',
  '@lezer/highlight',
  '@codemirror/view',
])
  dependencies[name] = await import(name);
const require = (name) => dependencies[name];
vm.runInNewContext(
  ts.transpileModule(readFileSync('src/plugins/files/editor-syntax.ts', 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS },
  }).outputText,
  { exports: syntaxExports, require },
);
const engine = await Ghostty.load(
  `data:application/wasm;base64,${readFileSync('node_modules/ghostty-web/ghostty-vt.wasm').toString('base64')}`,
);
const hex = (color) => parseInt(color.slice(1), 16);
const rgb = (cell, prefix) =>
  (cell[prefix + '_r'] << 16) | (cell[prefix + '_g'] << 8) | cell[prefix + '_b'];
for (const theme of themePresets) {
  const p = theme.terminal;
  const buffer = engine.createTerminal(80, 24, {
    fgColor: hex(p.foreground),
    bgColor: hex(p.background),
    cursorColor: hex(p.cursor),
    palette: ansiKeys.map((key) => hex(p[key])),
  });
  try {
    buffer.write('\x1bc');
    buffer.write('default ');
    for (let i = 0; i < 16; i++) buffer.write(`\x1b[${i < 8 ? 30 + i : 90 + i - 8}mX`);
    buffer.write('\x1b[38;2;18;52;86mRGB');
    const cells = buffer.getLine(0);
    assert.equal(rgb(cells[0], 'fg'), hex(p.foreground), theme.name);
    assert.equal(rgb(cells[0], 'bg'), hex(p.background), theme.name);
    for (let i = 0; i < 16; i++)
      assert.equal(rgb(cells[8 + i], 'fg'), hex(p[ansiKeys[i]]), `${theme.name}: ANSI ${i}`);
    assert.equal(rgb(cells[24], 'fg'), 0x123456);
  } finally {
    buffer.free();
  }
}
// Actual 0.4.0 renderer with canvas calls recorded; no browser certification.
const fills = [];
const ctx = {
  fillStyle: '',
  fillRect() {
    fills.push(this.fillStyle);
  },
  fillText() {},
  strokeRect() {},
  beginPath() {},
  moveTo() {},
  lineTo() {},
  stroke() {},
};
const renderer = Object.assign(Object.create(CanvasRenderer.prototype), {
  ctx,
  metrics: { width: 8, height: 16, baseline: 12 },
  theme: themePresets[0].terminal,
  currentSelectionCoords: null,
});
const cell = {
  codepoint: 65,
  fg_r: 18,
  fg_g: 52,
  fg_b: 86,
  bg_r: 0,
  bg_g: 0,
  bg_b: 0,
  flags: 0,
  width: 1,
  hyperlink_id: 0,
  grapheme_len: 0,
};
const frames = new Map();
let id = 0;
let visible = false;
globalThis.requestAnimationFrame = (callback) => {
  frames.set(++id, callback);
  return id;
};
globalThis.cancelAnimationFrame = (id) => frames.delete(id);
let paints = 0;
const term = {
  startRenderLoop() {},
  cursorMoveEmitter: { fire() {} },
  wasmTerm: {},
  write() {},
  renderer: {
    render() {
      paints++;
    },
    setHoveredHyperlinkId() {},
    setHoveredLinkRange() {},
  },
  onSelectionChange: () => ({ dispose() {} }),
  onScroll: () => ({ dispose() {} }),
};
const rendering = prepareTerminalRendering(term, () => visible);
rendering.opened();
for (const theme of themePresets) {
  renderer.setTheme(theme.terminal);
  renderer.renderLine([cell], 0, 1);
  assert.equal(fills.at(-1), theme.terminal.background);
  renderer.currentSelectionCoords = { startRow: 0, endRow: 0, startCol: 0, endCol: 1 };
  renderer.renderCellBackground(cell, 0, 0);
  assert.equal(fills.at(-1), theme.terminal.selectionBackground);
  renderer.currentSelectionCoords = null;
  assert.equal(renderer.theme.cursor, theme.terminal.cursor);
  assert.equal(renderer.palette.join(','), ansiKeys.map((key) => theme.terminal[key]).join(','));
  rendering.invalidate(true);
}
assert.equal(frames.size, 0, 'Hidden theme changes must not schedule frames');
visible = true;
rendering.visibilityChanged();
assert.equal(frames.size, 1);
for (const callback of frames.values()) callback();
frames.clear();
assert.equal(paints, 1);
rendering.dispose();
// Reconfigure the same extensions as Files without replacing document/history.
const compartment = new Compartment();
const extension = syntaxExports.editorAppearance;
let state = EditorState.create({
  doc: 'original',
  extensions: [history(), compartment.of(extension(themePresets[0]))],
});
state = state.update({ changes: { from: 8, insert: ' draft' }, selection: { anchor: 14 } }).state;
for (const theme of themePresets) {
  state = state.update({ effects: compartment.reconfigure(extension(theme)) }).state;
  assert.equal(state.doc.toString(), 'original draft');
  assert.equal(state.selection.main.head, 14);
  assert.equal(state.facet(EditorView.darkTheme), theme.mode === 'dark');
}
const target = {
  get state() {
    return state;
  },
  dispatch(tr) {
    state = tr.state;
  },
};
assert.equal(undo(target), true);
assert.equal(state.doc.toString(), 'original');
assert.equal(redo(target), true);
assert.equal(state.doc.toString(), 'original draft');
console.log(
  'Six launch palettes: default FG/BG, 16 ANSI and explicit RGB passed against Ghostty 0.4.0. Renderer decoration updates, hidden repaint and editor draft/selection/undo/redo preservation passed. Live WASM recoloring remains unsupported.',
);
