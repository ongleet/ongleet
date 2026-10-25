import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Ghostty } from 'ghostty-web';
import { attachTerminalMouse } from '../src/terminal-mouse.ts';
import { TerminalLinks } from '../src/terminal-links.ts';

const wasm = await readFile(
  new URL('../node_modules/ghostty-web/ghostty-vt.wasm', import.meta.url),
);
const ghostty = await Ghostty.load(`data:application/wasm;base64,${wasm.toString('base64')}`);
const engine = ghostty.createTerminal(80, 24);
const host = new EventTarget();
globalThis.document = new EventTarget();
host.querySelector = () => ({ getBoundingClientRect: () => ({ left: 10, top: 20 }) });
const term = {
  cols: 80,
  rows: 24,
  renderer: { charWidth: 10, charHeight: 20 },
  focus() {},
  getMode: (mode) => engine.getMode(mode),
  hasMouseTracking: () => engine.hasMouseTracking(),
};
const reports = [];
let enabled = true;
const adapter = attachTerminalMouse(
  host,
  term,
  (bytes) => reports.push(Buffer.from(bytes)),
  () => enabled,
);
function mouse(type, props = {}, target = host) {
  const event = new Event(type, { cancelable: true });
  Object.assign(event, {
    clientX: 35,
    clientY: 65,
    button: 0,
    buttons: 1,
    shiftKey: false,
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    ...props,
  });
  target.dispatchEvent(event);
  return event;
}
try {
  mouse('mousedown');
  assert.equal(reports.length, 0, 'Plain terminals retain local selection');
  engine.write('\x1b[?1002h\x1b[?1006h');
  assert.equal(mouse('mousedown').defaultPrevented, true);
  mouse('mousemove', { clientX: 45 });
  mouse('mousemove', { clientX: 46 });
  mouse('mouseup', {}, document);
  assert.deepEqual(
    reports.map((bytes) => bytes.toString()),
    ['\x1b[<0;3;3M', '\x1b[<32;4;3M', '\x1b[<0;3;3m'],
  );
  reports.length = 0;
  assert.equal(mouse('mousedown', { shiftKey: true }).defaultPrevented, false);
  mouse('mousemove', { shiftKey: true });
  mouse('mouseup', { shiftKey: true }, document);
  assert.equal(reports.length, 0, 'Shift-drag selects locally without PTY mouse reports');
  mouse('wheel', { deltaY: -40, deltaMode: 0, buttons: 0 });
  assert.equal(reports.pop().toString(), '\x1b[<64;3;3M');
  enabled = false;
  mouse('mousedown');
  assert.equal(reports.length, 0, 'Replay and hidden terminals cannot send mouse input');
  enabled = true;
  engine.write('\x1b[?1006l');
  engine.resize(120, 24);
  term.cols = 120;
  mouse('mousedown', { clientX: 1005 });
  assert.deepEqual(
    [...reports.pop()],
    [27, 91, 77, 32, 132, 35],
    'Legacy coordinates remain raw bytes, not UTF-8',
  );
  mouse('mouseup', {}, document);
  reports.length = 0;
  adapter.dispose();
  mouse('mousedown');
  assert.equal(reports.length, 0, 'Disposed adapters detach their input listeners');
  console.log(
    'PASS: real Ghostty mouse modes, SGR press/drag/release/wheel, local selection override, replay guard, raw legacy bytes and disposal',
  );
} finally {
  adapter.dispose();
  engine.free();
}

const screen = ghostty.createTerminal(8, 3);
const linkedTerm = { wasmTerm: screen };
const links = new TerminalLinks(linkedTerm, (bytes) => screen.write(Uint8Array.from(bytes)));
const write = (text) => links.write([...new TextEncoder().encode(text)]);
const uriAt = (row, col) => screen.getHyperlinkUri(screen.getLine(row)[col].hyperlink_id);
try {
  for (const byte of new TextEncoder().encode(
    '\x1b]8;id=first;https://example.com/été\x1b\\\x1b[32m日本\x1b]8;;\x07',
  ))
    links.write([byte]);
  assert.equal(
    uriAt(0, 0),
    'https://example.com/été',
    'Split Unicode OSC 8 links resolve using real Ghostty cell IDs',
  );
  write('\x1b]8;;file:///tmp/a%20b.txt\x07ABCDWRAP\x1b]8;;\x07');
  assert.equal(uriAt(0, 4), 'file:///tmp/a%20b.txt');
  assert.equal(
    uriAt(1, 0),
    'file:///tmp/a%20b.txt',
    'Wrapped links share their engine hyperlink ID',
  );
  write('\x1b[3;8H\x1b]8;;https://example.com/bottom\x07Z\x1b]8;;\x07');
  assert.equal(
    uriAt(2, 7),
    'https://example.com/bottom',
    'A final-column link resolves without cursor movement',
  );
  write('\x1b]8;;https://example.com/scroll\x07S\x1b]8;;\x07');
  assert.equal(
    uriAt(2, 0),
    'https://example.com/scroll',
    'Bottom-margin wrap binds the newly printed cell',
  );
  write('\x1bc');
  assert.equal(screen.getHyperlinkUri(1), null, 'RIS discards stale hyperlink metadata');
  console.log(
    'PASS: OSC 8 URI metadata, split UTF-8/ST, consecutive links, wrapping, bottom-margin scroll and reset against real Ghostty cells',
  );
} finally {
  screen.free();
}

// Title metadata follows the same raw, arbitrarily fragmented PTY stream as links.
const titleScreen = ghostty.createTerminal(80, 24);
const titles = [];
const titleStream = new TerminalLinks(
  { wasmTerm: titleScreen },
  (bytes) => titleScreen.write(Uint8Array.from(bytes)),
  (title) => titles.push(title),
);
try {
  const bytes = new TextEncoder().encode('\x1b]0;alice@host: ~/été\x07$ \x1b]2;vim main.ts\x1b\\');
  for (const byte of bytes) titleStream.write([byte]);
  assert.deepEqual(titles, ['alice@host: ~/été', 'vim main.ts']);
  titleStream.write([...new TextEncoder().encode('\x1b]1;icon only\x07\x1b]2;cancelled\x18')]);
  assert.equal(titles.length, 2, 'Icon titles and cancelled sequences leave the tab unchanged');
  titleStream.write([...new TextEncoder().encode('\x1b]2;' + 'x'.repeat(8193) + '\x07')]);
  assert.equal(titles.length, 2, 'Oversized OSC metadata is discarded');
  titleStream.write([...new TextEncoder().encode('\x1b]2;\x07\x1bc')]);
  assert.deepEqual(
    titles.slice(-2),
    ['', null],
    'Empty title and reset restore the configured name',
  );
  console.log('PASS: fragmented UTF-8 shell titles, BEL/ST, cancellation, bounds and reset');
} finally {
  titleScreen.free();
}
