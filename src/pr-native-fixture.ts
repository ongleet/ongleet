// Loaded exclusively by native-validation builds. Dummy GitHub responses;
// credentials still cross the real native Secret Service boundary.
import type { PluginSdk, HttpRequest } from './sdk';
type Mode = 'ok' | 'wait' | 'permission' | 'offline';
let calls = 0;
let active = 0;
const modeKey = 'ongleet.validation.pr-fixture';
Object.assign(window, {
  __prFixture: {
    enable: (mode: Mode = 'ok') => localStorage.setItem(modeKey, mode),
    mode: (mode: Mode) => localStorage.setItem(modeKey, mode),
    calls: () => calls,
    active: () => active,
  },
});
export function installPrFixture(sdk: PluginSdk) {
  const nativeRequest = sdk.http.request;
  sdk.http.request = async (request: HttpRequest) => {
    const mode = localStorage.getItem(modeKey);
    if (!mode || request.url !== 'https://api.github.com/graphql') return nativeRequest(request);
    ++calls;
    const result = (data: unknown, status = 200) => ({
      status,
      headers: {},
      body: JSON.stringify(data),
    });
    if (request.headers?.Authorization === 'Bearer invalid-fixture') return result({}, 401);
    const { query } = JSON.parse(request.body!);
    if (query.includes('viewer'))
      return result({
        data: {
          viewer: {
            login:
              request.headers?.Authorization === 'Bearer second-fixture'
                ? 'second-user'
                : 'fixture-user',
          },
        },
      });
    if (mode === 'wait') {
      ++active;
      try {
        await new Promise<void>((_, reject) => {
          const abort = () => reject(new Error('Fixture HTTP cancelled'));
          if (request.signal?.aborted) abort();
          else request.signal?.addEventListener('abort', abort, { once: true });
        });
      } finally {
        --active;
      }
    }
    if (mode === 'offline') throw new Error('Fixture network offline');
    if (mode === 'permission') return result({ errors: [{ type: 'FORBIDDEN' }] });
    const pageInfo = { hasNextPage: false, endCursor: null };
    return result({
      data: {
        repository: {
          pullRequest: {
            id: 'fixture-pr',
            title: 'Plugin native fixture',
            url: 'https://github.com/example/repo/pull/42',
            number: 42,
            state: 'OPEN',
            reviewDecision: 'APPROVED',
            reviews: {
              nodes: [
                {
                  id: 'review-graphql-id',
                  author: { login: 'reviewer' },
                  state: 'APPROVED',
                  submittedAt: '2026-10-25T12:00:00Z',
                },
              ],
              pageInfo,
            },
            commits: {
              nodes: [
                {
                  commit: {
                    id: 'fixture-commit',
                    statusCheckRollup: {
                      contexts: { nodes: [{ name: 'build', conclusion: 'SUCCESS' }], pageInfo },
                    },
                  },
                },
              ],
            },
          },
        },
      },
    });
  };
}
