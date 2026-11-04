import type { SelectedCode } from '../../components/tab-send';
/** Read only selected code cells: gutters, discussion widgets and controls are excluded. */
export function captureDiffSelection(
  viewer: HTMLElement,
  tabId: number,
  root: string,
  revision: string,
): SelectedCode {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount !== 1)
    throw new Error('Select some code in one diff file first.');
  const range = selection.getRangeAt(0);
  if (!viewer.contains(range.startContainer) || !viewer.contains(range.endContainer))
    throw new Error('Keep the selection inside one diff file.');
  const codeCell = (node: Node) =>
    (node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement)?.closest(
      'td.diff-code',
    );
  if (!codeCell(range.startContainer) || !codeCell(range.endContainer))
    throw new Error('Start and end the selection in code text.');
  const selected: { cell: HTMLElement; text: string; location: string }[] = [];
  for (const cell of viewer.querySelectorAll<HTMLElement>('td.diff-code')) {
    if (!range.intersectsNode(cell)) continue;
    const part = document.createRange();
    part.selectNodeContents(cell);
    if (range.compareBoundaryPoints(Range.START_TO_START, part) > 0)
      part.setStart(range.startContainer, range.startOffset);
    if (range.compareBoundaryPoints(Range.END_TO_END, part) < 0)
      part.setEnd(range.endContainer, range.endOffset);
    const text = part.toString();
    if (!text) continue;
    const location = [...cell.closest('tr')!.querySelectorAll<HTMLElement>('[data-comment-side]')]
      .map((node) => `${node.dataset.commentSide}:${node.dataset.commentLine}`)
      .join(' / ');
    selected.push({ cell, text, location });
  }
  if (!selected.length) throw new Error('Select code text, rather than gutters or comments.');
  const file = selected[0].cell.closest<HTMLElement>('[data-diff-path]')!;
  if (selected.some((item) => item.cell.closest('[data-diff-path]') !== file))
    throw new Error('Select text from one diff file at a time.');
  const cells = [...file.querySelectorAll<HTMLElement>('td.diff-code')];
  const first = cells.indexOf(selected[0].cell),
    last = cells.indexOf(selected[selected.length - 1].cell);
  return {
    source: 'Diff',
    tabId,
    path: `${root}/${file.dataset.diffPath}`,
    revision,
    location:
      selected.length === 1
        ? selected[0].location
        : `${selected[0].location} through ${selected[selected.length - 1].location}`,
    text: selected.map((item) => item.text).join('\n'),
    context: cells
      .slice(Math.max(0, first - 2), last + 3)
      .map(
        (cell) =>
          `${cell.classList.contains('diff-code-delete') ? '-' : cell.classList.contains('diff-code-insert') ? '+' : ' '}${cell.textContent}`,
      )
      .join('\n')
      .slice(0, 8192),
  };
}
