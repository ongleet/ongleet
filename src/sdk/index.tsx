import { createContext, useContext, useEffect, useRef } from 'react';
import type { createSdk } from './transport';
export type PluginSdk = ReturnType<typeof createSdk>;
export * from './types';
export const SdkContext = createContext<PluginSdk | null>(null);
export function useSdk(): PluginSdk {
  const sdk = useContext(SdkContext);
  if (!sdk) throw new Error('Plugin must be mounted by its SDK host');
  return sdk;
}

export type { ThemePreset, ThemeId } from '../theme-presets';

/** Retains one registration while handlers read the latest committed component state. */
export function useTabInteractions(handlers: import('./types').InteractionHandlers) {
  const sdk = useSdk();
  const current = useRef(handlers);
  useEffect(() => {
    current.current = handlers;
  });
  useEffect(
    () =>
      sdk.tab.registerInteractions({
        state: handlers.state
          ? (args, context) => current.current.state!(args, context)
          : undefined,
        actions: Object.fromEntries(
          Object.keys(handlers.actions ?? {}).map((name) => [
            name,
            (args: Record<string, unknown>, context: { signal: AbortSignal }) =>
              current.current.actions![name](args, context),
          ]),
        ),
      }),
    [sdk],
  );
}
