import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { CanvasRenderer, Ghostty } from 'ghostty-web';
import { prepareTerminalRendering } from '../src/terminal-rendering.ts';

const bytes = await readFile(
  new URL('../node_modules/ghostty-web/ghostty-vt.wasm', import.meta.url),
);
const ghostty = await Ghostty.load(`data:application/wasm;base64,${bytes.toString('base64')}`);
const buffer = ghostty.createTerminal(80, 24);
const frames = new Map();
let nextFrame = 0;
globalThis.requestAnimationFrame = (callback) => {
  frames.set(++nextFrame, callback);
  return nextFrame;
};
globalThis.cancelAnimationFrame = (id) => frames.delete(id);
function flush() {
  const pending = [...frames.values()];
  frames.clear();
  pending.forEach((callback) => callback(0));
}
let visible = true;
let paints = 0;
let rows = 0;
const listeners = new Set();
const subscribe = (callback) => {
  listeners.add(callback);
  return { dispose: () => listeners.delete(callback) };
};
// Real pinned renderer and WASM; replace only DOM/canvas drawing and the clock.
const renderer = Object.assign(Object.create(CanvasRenderer.prototype), {
  canvas: { width: 80, height: 24 },
  metrics: { width: 1, height: 1 },
  devicePixelRatio: 1,
  lastCursorPosition: { x: 0, y: 0 },
  lastViewportY: 0,
  cursorVisible: true,
  cursorBlink: false,
  hoveredHyperlinkId: 0,
  previousHoveredHyperlinkId: 0,
  hoveredLinkRange: null,
  previousHoveredLinkRange: null,
  renderLine() {
    rows++;
  },
  renderCursor() {},
  renderScrollbar() {},
});
const draw = renderer.render.bind(renderer);
renderer.render = (...args) => {
  paints++;
  draw(...args);
};
const term = {
  renderer,
  wasmTerm: buffer,
  viewportY: 0,
  startRenderLoop() {
    throw new Error('Idle render loop started');
  },
  lastCursorY: 0,
  cursorMoveEmitter: { fire() {} },
  scrollbarOpacity: 0,
  write: (data) => buffer.write(data),
  getScrollbackLength: () => buffer.getScrollbackLength(),
  getScrollbackLine: (i) => buffer.getScrollbackLine(i),
  onSelectionChange: subscribe,
  onScroll: subscribe,
};
const rendering = prepareTerminalRendering(term, () => visible);
try {
  term.startRenderLoop();
  rendering.opened();
  term.write(new TextEncoder().encode('history line\r\n'.repeat(100)));
  term.write(new TextEncoder().encode('final marker'));
  assert.equal(frames.size, 1, 'Burst writes must coalesce');
  flush();
  assert.equal(paints, 1);
  assert.equal(frames.size, 0, 'Settled output must leave no frame scheduled');
  term.viewportY = 10;
  listeners.forEach((callback) => callback());
  flush();
  assert.equal(rows, 48, 'Returning to history must paint all visible rows');
  const settled = paints;
  for (let i = 0; i < 60; i++) flush();
  assert.equal(paints, settled, 'Unchanged scrollback must not repaint');
  renderer.setHoveredHyperlinkId(3);
  assert.equal(frames.size, 1, 'Hover still invalidates without the upstream loop');
  visible = false;
  rendering.visibilityChanged();
  assert.equal(frames.size, 0, 'Hiding cancels pending drawing');
  term.write(new TextEncoder().encode(' hidden update'));
  assert.equal(frames.size, 0, 'Hidden output must parse without scheduling a frame');
  assert.match(
    buffer
      .getLine(23)
      .map((c) => String.fromCodePoint(c.codepoint || 32))
      .join(''),
    /hidden update/,
  );
  visible = true;
  rendering.visibilityChanged();
  flush();
  assert.equal(paints, settled + 1, 'Returning must redraw hidden changes');
  term.viewportY = 0;
  listeners.forEach((callback) => callback());
  flush();
  rows = 0;
  term.write(new TextEncoder().encode('!'));
  flush();
  assert.ok(rows > 0 && rows < 24, 'Small output must retain dirty-row painting');
  renderer.setHoveredLinkRange(null);
  rendering.dispose();
  assert.equal(frames.size, 0, 'Disposal cancels pending drawing');
  assert.equal(listeners.size, 0);
  console.log(
    'PASS: coalesced output, no idle/scrollback frames, hidden parsing, hover, visibility and disposal',
  );
} finally {
  rendering.dispose();
  buffer.free();
}
