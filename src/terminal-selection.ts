import type { Terminal } from 'ghostty-web';

// Read the existing Ghostty selection. Upstream 0.4.0 serializes wide-cell
// continuation slots as spaces; those slots are not additional text.
type Point = { col: number; absoluteRow: number };

type Manager = {
  boundClickHandler: (event: MouseEvent) => void;
  selectionStart: Point | null;
  selectionEnd: Point | null;
  autoScrollDirection: number;
  startAutoScroll: (direction: number) => void;
  stopAutoScroll: () => void;
  getSelection: () => string;
  hasSelection: () => boolean;
};

function managerFor(term: Terminal) {
  return (term as unknown as { selectionManager?: Manager }).selectionManager;
}

// In 0.4.0 an edge-scroll timer extends selection even when the viewport cannot
// move. Keep Ghostty's selection logic, but stop its timer at scroll boundaries.
export function attachTerminalSelection(term: Terminal) {
  const manager = managerFor(term);
  if (!manager || typeof manager.boundClickHandler !== 'function')
    throw new Error('Unsupported Ghostty selection API');
  // Pinned 0.4.0 clears selections on every click outside its own canvas.
  // Navigation must retain each terminal's selection, so remove only that
  // document listener; canvas selection and keyboard clearing stay upstream.
  const outsideClick = manager.boundClickHandler;
  document.removeEventListener('click', outsideClick);
  const start = manager.startAutoScroll;
  const getSelection = manager.getSelection;
  // Upstream's automatic/browser copy must use the same Unicode text as the
  // native clipboard path, rather than racing it with continuation spaces.
  manager.getSelection = () => (manager.hasSelection() ? terminalSelectionText(term) : '');
  let disposed = false;
  const atEdge = (direction: number) =>
    direction < 0
      ? term.viewportY >= (term.wasmTerm?.getScrollbackLength() ?? 0)
      : direction > 0 && term.viewportY <= 0;
  manager.startAutoScroll = (direction) => {
    if (atEdge(direction)) manager.stopAutoScroll();
    else start.call(manager, direction);
  };
  const subscription = term.onScroll(() => {
    // Let the current timer tick finish before stopping it; that tick still
    // reads autoScrollDirection after emitting the synchronous scroll event.
    queueMicrotask(() => {
      if (!disposed && atEdge(manager.autoScrollDirection)) manager.stopAutoScroll();
    });
  });
  return {
    dispose() {
      disposed = true;
      subscription.dispose();
      manager.stopAutoScroll();
      manager.startAutoScroll = start;
      manager.getSelection = getSelection;
      document.addEventListener('click', outsideClick);
    },
  };
}

export function terminalSelectionText(term: Terminal) {
  // In pinned ghostty-web 0.4.0 the public range is clipped to the viewport.
  // Copy needs the original absolute endpoints, including offscreen history.
  const manager = managerFor(term);
  if (!term.wasmTerm || !manager?.selectionStart || !manager.selectionEnd) return '';
  return ghosttySelectionText(term.wasmTerm, manager.selectionStart, manager.selectionEnd);
}

export function ghosttySelectionText(
  engine: NonNullable<Terminal['wasmTerm']>,
  first: Point,
  last: Point,
) {
  let startPoint = first;
  let endPoint = last;
  if (
    first.absoluteRow > last.absoluteRow ||
    (first.absoluteRow === last.absoluteRow && first.col > last.col)
  )
    [startPoint, endPoint] = [last, first];
  const scrollback = engine.getScrollbackLength();
  const lines: string[] = [];
  for (let row = startPoint.absoluteRow; row <= endPoint.absoluteRow; row++) {
    const history = row < scrollback;
    const index = history ? row : row - scrollback;
    const cells = history ? engine.getScrollbackLine(index) : engine.getLine(index);
    if (!cells) continue;
    let start = row === startPoint.absoluteRow ? startPoint.col : 0;
    const end = Math.min(
      row === endPoint.absoluteRow ? endPoint.col : cells.length - 1,
      cells.length - 1,
    );
    if (start > 0 && cells[start]?.width === 0 && cells[start - 1]?.width === 2) start--;
    let text = '';
    for (let col = start; col <= end; col++) {
      const cell = cells[col];
      if (!cell || cell.width === 0) continue;
      if (!cell.codepoint) text += ' ';
      else if (cell.grapheme_len > 0)
        text += history
          ? engine.getScrollbackGraphemeString(index, col)
          : engine.getGraphemeString(index, col);
      else text += String.fromCodePoint(cell.codepoint);
    }
    lines.push(text.trimEnd());
  }
  return lines.join('\n');
}
