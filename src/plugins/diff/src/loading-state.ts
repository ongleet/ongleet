import { createContext } from 'react';

/** Notify loading subscribers without invalidating every memoized patch viewer. */
export function createLoadingState() {
  let value = false;
  const listeners = new Set<() => void>();
  return {
    getSnapshot: () => value,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    set: (next: boolean) => {
      if (next === value) return;
      value = next;
      for (const listener of listeners) listener();
    },
  };
}

export const DiffLoading = createContext<ReturnType<typeof createLoadingState> | null>(null);
