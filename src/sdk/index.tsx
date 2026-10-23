import { createContext, useContext } from 'react';
import type { createSdk } from './transport';
export type PluginSdk = ReturnType<typeof createSdk>;
export * from './types';
export const SdkContext = createContext<PluginSdk | null>(null);
export function useSdk(): PluginSdk {
  const sdk = useContext(SdkContext);
  if (!sdk) throw new Error('Plugin must be mounted by its SDK host');
  return sdk;
}
