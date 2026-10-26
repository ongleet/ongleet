import type { Terminal } from 'ghostty-web';
import { suppressHiddenRendering } from './render-visibility.ts';

// Compatibility boundary for pinned ghostty-web 0.4.0. Upstream has no API to
// disable its perpetual frame loop. Replace it before open(), retaining its
// own finite scroll/scrollbar animations and synchronous VT parsing.
export function prepareTerminalRendering(term: Terminal, visible: () => boolean) {
  const internals = term as unknown as {
    startRenderLoop(): void;
    lastCursorY: number;
    cursorMoveEmitter: { fire(): void };
    scrollbarOpacity: number;
  };
  if (typeof internals.startRenderLoop !== 'function' || !internals.cursorMoveEmitter)
    throw new Error('Unsupported Ghostty rendering API');
  internals.startRenderLoop = () => {};
  let frame: number | undefined;
  let disposed = false;
  let fullRedraw = false;
  const invalidate = (forceAll = false) => {
    if (disposed) return;
    fullRedraw ||= forceAll;
    if (frame !== undefined || !visible()) return;
    frame = requestAnimationFrame(() => {
      frame = undefined;
      if (!disposed && visible() && term.wasmTerm) {
        // Preserve upstream dirty-row rendering for output. Selection, hover
        // and visibility changes also need to erase previously painted pixels.
        term.renderer?.render(
          term.wasmTerm,
          fullRedraw,
          term.viewportY,
          term,
          internals.scrollbarOpacity,
        );
        fullRedraw = false;
      }
    });
  };
  const write = term.write.bind(term);
  term.write = (...args) => {
    write(...args);
    const cursor = term.wasmTerm?.getCursor();
    if (cursor && cursor.y !== internals.lastCursorY) {
      internals.lastCursorY = cursor.y;
      internals.cursorMoveEmitter.fire();
    }
    invalidate();
  };
  const subscriptions: { dispose(): void }[] = [];
  return {
    opened() {
      const renderer = term.renderer;
      if (!renderer) throw new Error('Ghostty renderer unavailable');
      suppressHiddenRendering(renderer, visible);
      const hoverId = renderer.setHoveredHyperlinkId.bind(renderer);
      renderer.setHoveredHyperlinkId = (id) => {
        hoverId(id);
        invalidate(true);
      };
      const hoverRange = renderer.setHoveredLinkRange.bind(renderer);
      renderer.setHoveredLinkRange = (range) => {
        hoverRange(range);
        invalidate(true);
      };
      subscriptions.push(
        term.onSelectionChange(() => invalidate(true)),
        term.onScroll(() => invalidate(true)),
      );
      invalidate(true);
    },
    visibilityChanged() {
      if (frame !== undefined) cancelAnimationFrame(frame);
      frame = undefined;
      invalidate(true);
    },
    dispose() {
      disposed = true;
      if (frame !== undefined) cancelAnimationFrame(frame);
      subscriptions.forEach((subscription) => subscription.dispose());
    },
  };
}
