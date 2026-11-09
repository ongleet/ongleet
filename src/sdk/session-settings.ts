import { useCallback, useEffect, useRef, useState } from 'react';
import type { PluginSdk } from './index';
import type { Namespace } from './types';

/** Settings belong to the calling plugin type and session, independently of tab data. */
export function useSessionSettings<T extends object>(sdk: PluginSdk, defaults: T) {
  const [saved, setSaved] = useState<Namespace<Partial<T>> | null>(null);
  const [global, setGlobal] = useState<Namespace<Partial<T>> | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const generation = useRef(0);
  useEffect(() => {
    let active = true;
    let unsubscribe: (() => void) | undefined;
    const load = async () => {
      const current = ++generation.current;
      try {
        const [next, global] = await Promise.all([
          sdk.storage.read<Partial<T>>('settings'),
          sdk.storage.read<Partial<T>>('globalSettings'),
        ]);
        if (active && current === generation.current) {
          setSaved(next);
          setGlobal(global);
          setError('');
        }
      } catch (error) {
        if (active && current === generation.current) setError(String(error));
      } finally {
        if (active && current === generation.current) setLoading(false);
      }
    };
    void sdk.storage
      .subscribe(() => void load())
      .then((stop) => {
        if (active) {
          unsubscribe = stop;
          void load();
        } else stop();
      })
      .catch((error) => {
        if (active) {
          setError(String(error));
          setLoading(false);
        }
      });
    return () => {
      active = false;
      ++generation.current;
      unsubscribe?.();
    };
  }, [sdk]);
  const patch = useCallback(
    async (
      changes: Partial<T>,
      scope: 'settings' | 'globalSettings' = 'settings',
      reset = false,
    ) => {
      const current = await sdk.storage.read<Partial<T>>(scope);
      const next = await sdk.storage.replace<Partial<T>>(
        scope,
        current?.revision ?? 0,
        1,
        reset
          ? changes
          : ({
              ...current?.value,
              ...changes,
            } as T),
      );
      ++generation.current;
      if (scope === 'globalSettings') setGlobal(next);
      else setSaved(next);
      setError('');
    },
    [sdk],
  );
  return {
    value: { ...defaults, ...global?.value, ...saved?.value },
    globalValue: { ...defaults, ...global?.value },
    overrides: saved?.value ?? {},
    loading,
    error,
    patch,
  };
}
