import { useEffect, useLayoutEffect, useRef } from 'react';
import type { Comparison, FileChange } from '../git';
import type { DiffSelection } from '../selection-state';

/** Preserve the reader’s viewport across refreshes and track the visible sidebar file. */
export function useDiffScroll(
  comparison: Comparison | undefined,
  visible: boolean,
  selection: DiffSelection,
) {
  const fileElements = useRef(new Map<string, HTMLElement>());
  const viewer = useRef<HTMLDivElement>(null);
  const refreshAnchor = useRef<{ key: string; offset: number; left: number } | null>(null);
  useLayoutEffect(() => {
    const anchor = refreshAnchor.current;
    refreshAnchor.current = null;
    const container = viewer.current;
    const element = anchor && fileElements.current.get(anchor.key);
    if (!anchor || !container || !element) return;
    container.scrollTop +=
      element.getBoundingClientRect().top - container.getBoundingClientRect().top - anchor.offset;
    container.scrollLeft = anchor.left;
  }, [comparison]);
  useEffect(() => {
    const container = viewer.current;
    if (!visible || !comparison || !container) return;
    let frame = 0;

    const update = () => {
      frame = 0;
      const top = container.getBoundingClientRect().top + 4;
      // Articles follow comparison order; locate the viewport in logarithmic reads.
      let low = 0;
      let high = comparison.files.length;
      while (low < high) {
        const middle = (low + high) >>> 1;
        const element = fileElements.current.get(comparison.files[middle].key);
        if (element && element.getBoundingClientRect().bottom > top) high = middle;
        else low = middle + 1;
      }
      const current = comparison.files[low];
      if (current) selection.set(current.key);
    };

    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    container.addEventListener('scroll', schedule, { passive: true });
    const observer = new ResizeObserver(schedule);
    observer.observe(container);
    if (container.firstElementChild) observer.observe(container.firstElementChild);
    schedule();
    return () => {
      container.removeEventListener('scroll', schedule);
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [comparison, visible]);

  /** Select a sidebar file and scroll its patch into view. */
  function select(file: FileChange) {
    selection.set(file.key);
    fileElements.current.get(file.key)?.scrollIntoView({ block: 'start' });
  }

  /** Record the current viewport only when the replacement comparison is ready to commit. */
  function captureAnchor(next: Comparison) {
    // Capture at commit time: the reader may have scrolled while Git was running.
    const container = viewer.current;
    if (container && container.clientHeight) {
      const top = container.getBoundingClientRect().top;
      const nextKeys = new Set(next.files.map((file) => file.key));
      const anchor = [...fileElements.current].find(
        ([key, element]) => nextKeys.has(key) && element.getBoundingClientRect().bottom > top + 4,
      );
      if (anchor)
        refreshAnchor.current = {
          key: anchor[0],
          offset: anchor[1].getBoundingClientRect().top - top,
          left: container.scrollLeft,
        };
    }
  }
  return { fileElements, viewer, select, captureAnchor };
}
