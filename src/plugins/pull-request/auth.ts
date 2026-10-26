import type { PluginSdk } from '../../sdk';
import { fetchViewer, GitHubError } from './github.ts';
type Credential = { token: string; login: string };
export type AuthState = {
  login?: string;
  version: number;
  loading: boolean;
  busy: boolean;
  paused: boolean;
  error: string;
  retryAt: number;
};
export function createAuthController() {
  let credential: Credential | undefined;
  let state: AuthState = {
    version: 0,
    loading: true,
    busy: false,
    paused: false,
    error: '',
    retryAt: 0,
  };
  const listeners = new Map<() => void, PluginSdk>();
  const requests = new Map<AbortController, PluginSdk>();
  let lifetime = 0;
  const publish = (patch: Partial<AuthState>) => {
    state = { ...state, ...patch };
    for (const listener of listeners.keys()) listener();
  };
  const abort = () => {
    for (const request of requests.keys()) request.abort();
    requests.clear();
  };
  function changed(next?: Credential) {
    abort();
    credential = next;
    publish({
      login: next?.login,
      version: state.version + 1,
      paused: false,
      error: '',
      retryAt: 0,
    });
  }
  return {
    snapshot: () => state,
    subscribe(sdk: PluginSdk, listener: () => void) {
      const first = listeners.size === 0;
      listeners.set(listener, sdk);
      if (first) {
        const current = ++lifetime;
        publish({ loading: true });
        void sdk.credentials
          .get('github-account')
          .then((raw) => {
            if (current !== lifetime) return;
            let saved: Credential | undefined;
            if (raw) {
              try {
                saved = JSON.parse(raw);
              } catch {
                throw new Error('Invalid saved GitHub credential');
              }
              if (
                typeof saved?.token !== 'string' ||
                !saved.token ||
                typeof saved.login !== 'string'
              )
                throw new Error('Invalid saved GitHub credential');
            }
            changed(saved);
          })
          .catch((error) => {
            if (current === lifetime) publish({ error: String(error) });
          })
          .finally(() => {
            if (current === lifetime) publish({ loading: false });
          });
      }
      listener();
      return () => {
        listeners.delete(listener);
        if (![...listeners.values()].includes(sdk)) {
          for (const [request, owner] of requests) {
            if (owner === sdk) {
              request.abort();
              requests.delete(request);
            }
          }
        }
        if (!listeners.size) {
          ++lifetime;
          abort();
          credential = undefined;
          state = {
            version: state.version + 1,
            loading: true,
            busy: false,
            paused: false,
            error: '',
            retryAt: 0,
          };
        }
      };
    },
    async connect(sdk: PluginSdk, token: string) {
      if (state.busy || state.loading) return;
      token = token.trim();
      if (!token || /\s/.test(token)) throw new Error('Enter a GitHub personal access token');
      const current = lifetime;
      const controller = new AbortController();
      requests.set(controller, sdk);
      publish({ busy: true, error: '' });
      try {
        const login = await fetchViewer(sdk, token, controller.signal);
        if (current !== lifetime || controller.signal.aborted) return;
        const next = { token, login };
        await sdk.credentials.set('github-account', JSON.stringify(next));
        if (current === lifetime) changed(next);
      } catch (error) {
        if (current === lifetime) publish({ error: String(error) });
      } finally {
        requests.delete(controller);
        if (current === lifetime) publish({ busy: false });
      }
    },
    async disconnect(sdk: PluginSdk) {
      if (state.busy || state.loading) return;
      const current = lifetime;
      publish({ busy: true, error: '' });
      abort();
      try {
        await sdk.credentials.delete('github-account');
        if (current === lifetime) changed();
      } catch (error) {
        if (current === lifetime) publish({ error: String(error) });
      } finally {
        if (current === lifetime) publish({ busy: false });
      }
    },
    async run<T>(
      sdk: PluginSdk,
      operation: (
        token: string,
        login: string,
        signal: AbortSignal,
        transport: PluginSdk,
      ) => Promise<T>,
    ): Promise<T> {
      if (!credential || state.paused || state.loading || state.busy)
        throw new Error('Connect a valid GitHub token to refresh.');
      if (Date.now() < state.retryAt)
        throw new Error(
          `GitHub rate limit; retry after ${new Date(state.retryAt).toLocaleTimeString()}.`,
        );
      const current = state.version;
      const controller = new AbortController();
      requests.set(controller, sdk);
      try {
        const transport: PluginSdk = {
          ...sdk,
          http: {
            request: async (options) => {
              if (Date.now() < state.retryAt)
                throw new GitHubError(
                  'rate',
                  'GitHub rate limit; polling paused until reset.',
                  state.retryAt,
                );
              const response = await sdk.http.request(options);
              if (current === state.version && response.headers['x-ratelimit-remaining'] === '0') {
                const reset = Number(response.headers['x-ratelimit-reset']) * 1000;
                if (Number.isFinite(reset) && reset > Date.now())
                  publish({ retryAt: Math.max(state.retryAt, reset) });
              }
              return response;
            },
          },
        };
        const result = await operation(
          credential.token,
          credential.login,
          controller.signal,
          transport,
        );
        if (current !== state.version || controller.signal.aborted)
          throw new Error('GitHub request cancelled');
        return result;
      } catch (error) {
        if (current === state.version && error instanceof GitHubError) {
          if (error.kind === 'authentication') {
            abort();
            publish({ paused: true, error: error.message });
          }
          if (error.kind === 'rate') publish({ retryAt: error.retryAt, error: error.message });
        }
        throw error;
      } finally {
        requests.delete(controller);
      }
    },
  };
}
export const githubAuth = createAuthController();
