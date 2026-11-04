import { installPrFixture } from './pr-native-fixture';
import type { PluginSdk } from './sdk';
// Imported only by the explicit native-validation Vite build. The normal
// application bundle exposes no testing bridge or terminal handles.
type Probe = {
  state: () => {
    sessionId?: number;
    replaying: boolean;
    cols?: number;
    rows?: number;
    scrollback?: number;
    viewportY?: number;
    selection?: string;
    alternate?: boolean;
  };
  snapshot: () => string;
  selectAll: () => void;
};
let connectionGate: { promise: Promise<void>; release: () => void; tabId?: number } | undefined;
export function waitForConnection(id: number) {
  if (connectionGate && connectionGate.tabId === undefined) {
    connectionGate.tabId = id;
    return connectionGate.promise;
  }
}
const terminals = new Map<number, Probe>();
export function registerTerminal(id: number, probe: Probe) {
  terminals.set(id, probe);
  return () => {
    if (terminals.get(id) === probe) terminals.delete(id);
  };
}
Object.assign(window, {
  __terminalValidation: {
    holdNextConnection: () => {
      if (connectionGate) throw new Error('A connection is already held');
      let release!: () => void;
      const promise = new Promise<void>((resolve) => {
        release = resolve;
      });
      connectionGate = { promise, release };
    },
    pendingConnection: () => connectionGate?.tabId,
    releaseConnection: () => {
      connectionGate?.release();
      connectionGate = undefined;
    },
    state: (id: number) => terminals.get(id)?.state(),
    snapshot: (id: number) => terminals.get(id)?.snapshot(),
    selectAll: (id: number) => terminals.get(id)?.selectAll(),
  },
});

const interactionModes = new Map<string, string>();
const interactionCounts = { started: 0, aborted: 0 };
const sdks = new Map<number, PluginSdk>();
export function registerSdk(id: number, sdk: PluginSdk) {
  installPrFixture(sdk);
  const register = sdk.tab.registerInteractions;
  sdk.tab.registerInteractions = (handlers) => {
    const wrap =
      (
        operation: string,
        handler: import('./sdk').InteractionHandler,
      ): import('./sdk').InteractionHandler =>
      async (args, context) => {
        const mode = interactionModes.get(`${id}:${operation}`);
        if (mode === 'invalid') return { invalid: true };
        if (mode === 'oversized') return 'x'.repeat(70000);
      if (mode === 'nonJson') return BigInt(1);
        if (mode === 'hold') {
          interactionCounts.started++;
          await new Promise<void>((resolve) =>
            context.signal.addEventListener(
              'abort',
              () => {
                interactionCounts.aborted++;
                resolve();
              },
              { once: true },
            ),
          );
          return null;
        }
        return handler(args, context);
      };
    return register({
      state: handlers.state && wrap('state', handlers.state),
      actions: Object.fromEntries(
        Object.entries(handlers.actions ?? {}).map(([name, handler]) => [
          name,
          wrap(name, handler),
        ]),
      ),
    });
  };
  sdks.set(id, sdk);
  return () => {
    if (sdks.get(id) === sdk) sdks.delete(id);
  };
}
Object.assign(window, {
  __interactionValidation: {
    mode: (id: number, operation: string, mode: string) =>
      interactionModes.set(`${id}:${operation}`, mode),
    counts: () => ({ ...interactionCounts }),
  },
  __sdkValidation: {
    ready: (id: number) => sdks.has(id),
    call: (
      id: number,
      service:
        | 'tabs'
        | 'tab'
        | 'session'
        | 'fs'
        | 'storage'
        | 'process'
        | 'pty'
        | 'desktop'
        | 'http'
        | 'credentials'
        | 'env',
      method: string,
      args: unknown[],
    ) => {
      const sdk = sdks.get(id);
      if (!sdk) throw new Error('SDK attachment unavailable');
      const operation = (sdk[service] as Record<string, (...args: never[]) => unknown>)[method];
      if (typeof operation !== 'function') throw new Error('Unknown SDK operation');
      return operation(...(args as never[]));
    },
  },
});
