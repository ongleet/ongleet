import { useEffect, useRef, useState } from 'react';

/** Briefly mark new openings, excluding the initial restored snapshot. */
export function useOpeningHighlight(ids: number[], ready = true) {
  const previous = useRef<Set<number> | null>(null);
  const [highlighted, setHighlighted] = useState<Set<number>>(new Set());
  const key = JSON.stringify(ids);
  useEffect(() => {
    if (!ready) return;
    const current = new Set<number>(JSON.parse(key));
    const added = new Set(
      [...current].filter((id) => previous.current && !previous.current.has(id)),
    );
    previous.current = current;
    setHighlighted((existing) => new Set([...existing, ...added].filter((id) => current.has(id))));
  }, [key, ready]);
  useEffect(() => {
    if (!highlighted.size) return;
    const timer = setTimeout(() => setHighlighted(new Set()), 1800);
    return () => clearTimeout(timer);
  }, [highlighted]);
  return highlighted;
}
