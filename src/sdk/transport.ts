import { themeAccess } from '../appearance';
// Private native boundary. Plugins import only the SDK public entrypoint.
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { Channel, invoke } from '@tauri-apps/api/core';
import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager';
import { open } from '@tauri-apps/plugin-dialog';
import {
  SdkError,
  type InteractionHandlers,
  type HttpRequest,
  type HttpResponse,
  type Attachment,
  type Listing,
  type Namespace,
  type ProcessRequest,
  type ProcessResult,
  type ProcessStatus,
  type PtyOutput,
  type Scope,
  type SessionTag,
  type SortOrder,
  type TextDocument,
} from './types';
function sdkError(error: unknown): SdkError {
  const structured = error as { code?: string; message?: string };
  return error instanceof SdkError
    ? error
    : new SdkError(structured?.code ?? 'operation_failed', structured?.message ?? String(error));
}
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw sdkError(error);
  }
}
export function createSdk(id: number, setTitle: (title: string | null) => void) {
  const attachment = crypto.randomUUID();
  let disposed = false;
  const ready = call<void>('sdk_attach', { id, attachment });
  let interactionCleanup: (() => void) | undefined;
  let interactionRegistration = Promise.resolve();
  const activeInteractions = new Map<number, AbortController>();
  async function interaction<T>(target: number, operation: string, args: unknown = {}): Promise<T> {
    return native<T>('sdk_interaction', { id, attachment, target, operation, args });
  }
  const subscriptions = new Set<UnlistenFn>();
  const channels = new Set<Channel<PtyOutput>>();
  async function request<T>(operation: string, fields: Record<string, unknown> = {}): Promise<T> {
    await ready;
    if (disposed) throw new SdkError('unavailable', 'Plugin attachment disposed');
    return call<T>('sdk_request', { id, attachment, request: { operation, ...fields } });
  }
  async function native<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
    await ready;
    if (disposed) throw new SdkError('unavailable', 'Plugin attachment disposed');
    return call<T>(command, args);
  }
  async function pty<T>(operation: string, fields: Record<string, unknown> = {}): Promise<T> {
    return native<T>('sdk_pty', { id, attachment, request: { operation, ...fields } });
  }
  async function desktop<T>(operation: () => Promise<T>): Promise<T> {
    await ready;
    if (disposed) throw new SdkError('unavailable', 'Plugin attachment disposed');
    try {
      return await operation();
    } catch (error) {
      throw sdkError(error);
    }
  }
  return {
    ready,
    theme: themeAccess,
    session: {
      addTag: (tag: SessionTag) => request<SessionTag[]>('addSessionTag', { tag }),
      removeTag: (name: string) => request<SessionTag[]>('removeSessionTag', { name }),
      listTags: () => request<SessionTag[]>('sessionTags'),
    },
    tabs: {
      list: (args: { sessionId?: number } = {}) =>
        interaction<{
          tabs: { id: number; sessionId: number; kind: string; name: string; isOpen: boolean }[];
        }>(0, 'list', args),
      capabilities: (tabId: number) =>
        interaction<import('./types').TabCapabilities>(tabId, 'capabilities'),
      state: (tabId: number, args: Record<string, unknown> = {}) =>
        interaction<{
          tab: { id: number; sessionId: number; kind: string; name: string };
          state: import('./types').TabState;
          responseMs: number;
        }>(tabId, 'state', args),
      invoke: (tabId: number, action: string, args: Record<string, unknown> = {}) => {
        if (['state', 'capabilities', 'list'].includes(action))
          return Promise.reject(new SdkError('unsupported_operation', 'Reserved interaction name'));
        return interaction<unknown>(tabId, action, args);
      },
    },
    tab: {
      registerInteractions: (handlers: InteractionHandlers) => {
        if (interactionCleanup) throw new SdkError('busy', 'Interactions already registered');
        let active = true;
        let unlisten: UnlistenFn | undefined;
        const cleanup = () => {
          if (!active) return;
          active = false;
          unlisten?.();
          for (const controller of activeInteractions.values()) controller.abort();
          activeInteractions.clear();
          interactionCleanup = undefined;
          interactionRegistration = interactionRegistration
            .then(async () => {
              await call('sdk_interaction_register', {
                id,
                attachment,
                enabled: false,
                hasState: false,
                actions: [],
              });
            })
            .catch(() => {});
        };
        interactionCleanup = cleanup;
        interactionRegistration = interactionRegistration
          .then(async () => {
            await ready;
            if (!active || disposed) return;
            unlisten = await listen<{
              requestId: number;
              operation: string;
              args: Record<string, unknown>;
              cancel?: number;
            }>(`tab-interaction-${id}-${attachment}`, (event) => {
              const payload = event.payload;
              if (payload.cancel !== undefined) {
                activeInteractions.get(payload.cancel)?.abort();
                activeInteractions.delete(payload.cancel);
                return;
              }
              if (!active || disposed) return;
              const controller = new AbortController();
              activeInteractions.set(payload.requestId, controller);
              const handler =
                payload.operation === 'state'
                  ? handlers.state
                  : handlers.actions?.[payload.operation];
              void (async () => {
                let result: unknown = null;
                let error: string | null = null;
                try {
                  if (!handler)
                    throw new SdkError('unavailable', 'Interaction handler unavailable');
                  result = await handler(payload.args, { signal: controller.signal });
                  if (result === undefined) result = null;
                  // Reject non-JSON provider values before native serialization can fail.
                  JSON.stringify(result);
                } catch (failure) {
                  result = null;
                  const mapped = sdkError(failure);
                  error = `${mapped.code}: ${mapped.message}`;
                }
                if (!active || disposed || controller.signal.aborted) return;
                activeInteractions.delete(payload.requestId);
                await call('sdk_interaction_reply', {
                  id,
                  attachment,
                  requestId: payload.requestId,
                  result,
                  error,
                }).catch(() => {});
              })();
            });
            if (!active || disposed) {
              unlisten();
              return;
            }
            await call('sdk_interaction_register', {
              id,
              attachment,
              enabled: true,
              hasState: !!handlers.state,
              actions: Object.keys(handlers.actions ?? {}),
            });
          })
          .catch((error) => {
            cleanup();
            console.error('Interaction registration failed', sdkError(error));
          });
        return cleanup;
      },
      setTitle: (title: string | null) => {
        if (disposed) throw new SdkError('unavailable', 'Plugin attachment disposed');
        if (title !== null && (typeof title !== 'string' || title.length > 256))
          throw new SdkError('limit_exceeded', 'Tab title must contain at most 256 characters');
        setTitle(title?.replace(/[\u0000-\u001f\u007f-\u009f]/g, '').trim() || null);
      },
    },
    fs: {
      list: (options: {
        folder: string;
        offset: number;
        order: SortOrder;
        showHidden: boolean;
        query: string;
      }) => request<Listing>('list', options),
      watchText: async (path: string, changed: () => void) => {
        await ready;
        if (disposed) throw new SdkError('unavailable', 'Plugin attachment disposed');
        const token = crypto.randomUUID();
        let active = true;
        const unlisten = await listen<string>(`fs-changed-${id}-${attachment}`, (event) => {
          if (active && !disposed && event.payload === token) changed();
        });
        const dispose = () => {
          if (!active) return;
          active = false;
          subscriptions.delete(dispose);
          unlisten();
          void call('sdk_request', {
            id,
            attachment,
            request: { operation: 'unwatchText', token },
          }).catch(() => {});
        };
        if (disposed) {
          dispose();
          throw new SdkError('unavailable', 'Plugin attachment disposed');
        }
        subscriptions.add(dispose);
        try {
          await request<void>('watchText', { path, token });
          if (!active || disposed) {
            await call('sdk_request', {
              id,
              attachment,
              request: { operation: 'unwatchText', token },
            });
          }
          return dispose;
        } catch (error) {
          dispose();
          throw sdkError(error);
        }
      },
      readText: (path: string) => request<TextDocument | null>('readText', { path }),
      saveText: (options: { path: string; text: string; revision: string }) =>
        request<TextDocument>('saveText', options),
    },
    storage: {
      subscribe: async (changed: () => void) => {
        await ready;
        if (disposed) throw new SdkError('unavailable', 'Plugin attachment disposed');
        let active = true;
        const unlisten = await listen('session-changed', () => {
          if (active && !disposed) changed();
        }).catch((error) => {
          throw sdkError(error);
        });
        const dispose = () => {
          active = false;
          subscriptions.delete(dispose);
          unlisten();
        };
        if (disposed) dispose();
        else subscriptions.add(dispose);
        return dispose;
      },
      configure: (patch: Record<string, unknown>) => request<void>('configure', { patch }),
      read: <T = unknown>(scope: Scope) => request<Namespace<T> | null>('readData', { scope }),
      replace: <T>(scope: Scope, revision: number, version: number, value: T) =>
        request<Namespace<T>>('replaceData', { scope, revision, version, value }),
    },
    env: {
      get: (name: string) => request<string | null>('environmentGet', { name }),
    },
    credentials: {
      get: (key: string) => request<string | null>('credentialGet', { key }),
      set: (key: string, value: string) => request<void>('credentialSet', { key, value }),
      delete: (key: string) => request<void>('credentialDelete', { key }),
    },
    http: {
      request: async ({ signal, ...options }: HttpRequest): Promise<HttpResponse> => {
        await ready;
        if (signal?.aborted) throw new SdkError('cancelled', 'HTTP cancelled');
        const token = crypto.randomUUID();
        let completed = false;
        const cancel = () => {
          // Invocation handlers can start out of order. Retry until the HTTP
          // handler observes cancellation, or detach takes over cleanup.
          void (async () => {
            while (!completed && !disposed) {
              try {
                await request('httpCancel', { token });
              } catch {
                return;
              }
              await new Promise((resolve) => setTimeout(resolve, 10));
            }
          })();
        };
        const result = request<HttpResponse>('http', { token, request: options });
        signal?.addEventListener('abort', cancel, { once: true });
        try {
          const response = await result;
          if (signal?.aborted) throw new SdkError('cancelled', 'HTTP cancelled');
          return response;
        } finally {
          completed = true;
          signal?.removeEventListener('abort', cancel);
        }
      },
    },
    process: {
      run: (options: ProcessRequest) => request<ProcessResult>('process', { request: options }),
      cancel: () => request<void>('cancel'),
    },
    desktop: {
      readText: () => desktop(readText),
      writeText: (text: string, selection: 'clipboard' | 'primary' = 'clipboard') =>
        selection === 'primary'
          ? native<void>('write_primary_selection', { text })
          : desktop(() => writeText(text)),
      clipboard: (selection: 'clipboard' | 'primary' = 'clipboard') =>
        native<{ kind: 'text'; text: string } | { kind: 'image' }>('terminal_clipboard', {
          primary: selection === 'primary',
        }),
      selectPath: (directory = false) => desktop(() => open({ directory, multiple: false })),
      openUrl: (uri: string) => native<void>('open_terminal_link', { uri }),
      openFile: (path: string) => request<void>('openFile', { path }),
    },
    pty: {
      connect: async (
        options: { cols: number; rows: number; startIfStopped: boolean },
        receive: (output: PtyOutput) => void,
      ) => {
        for (const channel of channels) channel.onmessage = () => {};
        channels.clear();
        const output = new Channel<PtyOutput>();
        channels.add(output);
        output.onmessage = (event) => {
          if (!disposed) receive(event);
        };
        return native<Attachment | null>('sdk_pty_connect', { id, attachment, ...options, output });
      },
      acknowledge: (runtimeId: number, seq: number) => pty<void>('acknowledge', { runtimeId, seq }),
      sendText: (runtimeId: number, text: string, options: { enter?: boolean } = {}) => {
        const bytes = new TextEncoder().encode(text + (options.enter ? '\r' : ''));
        if (bytes.length > 8192)
          return Promise.reject(
            new SdkError('limit_exceeded', 'Input exceeds 8 KiB including Enter'),
          );
        return pty<void>('input', { runtimeId, bytes: Array.from(bytes) });
      },
      input: (runtimeId: number, bytes: number[]) => pty<void>('input', { runtimeId, bytes }),
      resize: (runtimeId: number, cols: number, rows: number) =>
        pty<void>('resize', { runtimeId, cols, rows }),
      status: (runtimeId: number) => pty<ProcessStatus>('status', { runtimeId }),
      history: () => pty<string>('history'),
      saveHistory: (runtimeId: number, text: string) =>
        pty<void>('saveHistory', { runtimeId, text }),
      close: (runtimeId: number) => pty<void>('close', { runtimeId }),
    },
    dispose: async () => {
      interactionCleanup?.();
      disposed = true;
      setTitle(null);
      for (const unsubscribe of [...subscriptions]) unsubscribe();
      for (const channel of channels) channel.onmessage = () => {};
      channels.clear();
      await ready;
      await call<void>('sdk_detach', { id, attachment });
    },
  };
}
