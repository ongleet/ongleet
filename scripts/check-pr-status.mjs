import assert from 'node:assert/strict';
import { test } from 'node:test';
import { pullRequestStatus, updatePullRequestTag } from '../src/plugins/pull-request/status-tag.ts';
import { refreshSeconds } from '../src/plugins/pull-request/settings.ts';
import {
  otherReviews,
  unreadReviews,
  acknowledgeReviews,
} from '../src/plugins/pull-request/reviews.ts';

test('existing configurations default to two minutes and unsafe timer values are rejected', () => {
  assert.equal(refreshSeconds({}), 120);
  assert.equal(refreshSeconds({ refreshIntervalSeconds: 300 }), 300);
  for (const seconds of [0, -1, 29, 86401, 30.5, '120', null])
    assert.throws(() => refreshSeconds({ refreshIntervalSeconds: seconds }), /Refresh interval/);
});

const review = (id, login, submittedAt, state = 'APPROVED') => ({
  id,
  author: login ? { login } : null,
  submittedAt,
  state,
});

test('baseline excludes own, pending and deleted-author reviews; new submitted reviews stay unread', () => {
  const existing = review('old', 'colleague', '2026-10-25T10:00:00Z');
  const all = [
    existing,
    review('self', 'ME', '2026-10-25T10:05:00Z'),
    review('draft', 'colleague', null, 'PENDING'),
    review('deleted', null, '2026-10-25T10:06:00Z'),
  ];
  const baseline = otherReviews(all, 'me');
  assert.deepEqual(baseline, [existing]);
  const saved = JSON.parse(JSON.stringify(acknowledgeReviews(baseline)));
  assert.deepEqual(unreadReviews(baseline, saved), []);
  const added = review('new', 'reviewer', '2026-10-25T10:10:00Z', 'CHANGES_REQUESTED');
  const refreshed = otherReviews([...all, added], 'me');
  assert.deepEqual(unreadReviews(refreshed, saved), [added]);
  assert.deepEqual(unreadReviews(refreshed, saved), [added]);
  const read = acknowledgeReviews(refreshed, saved);
  assert.deepEqual(unreadReviews(refreshed, read), []);
  assert.deepEqual(acknowledgeReviews([existing], read), read);
});

test('a second review at the same timestamp is new; empty baselines detect the first review', () => {
  const first = review('one', 'alice', '2026-10-25T10:00:00Z');
  const second = review('two', 'bob', '2026-10-25T10:00:00Z', 'COMMENTED');
  assert.deepEqual(unreadReviews([first], acknowledgeReviews([])), [first]);
  const cursor = acknowledgeReviews([first]);
  assert.deepEqual(unreadReviews([first, second], cursor), [second]);
  assert.deepEqual(unreadReviews([first, second], acknowledgeReviews([second], cursor)), []);
});

test('PR transitions and removal preserve sibling tags and avoid redundant writes', async () => {
  const sibling = { name: 'Codex', dot: '#123456', priority: 10 };
  let tags = [sibling];
  let writes = 0;
  const service = {
    async listTags() {
      return tags;
    },
    async addTag(tag) {
      writes++;
      tags = [...tags.filter((item) => item.name !== tag.name), tag];
      return tags;
    },
    async removeTag(name) {
      writes++;
      tags = tags.filter((tag) => tag.name !== name);
      return tags;
    },
  };
  for (const state of ['DRAFT', 'OPEN', 'CLOSED', 'MERGED', 'DRAFT']) {
    await updatePullRequestTag(service, state);
    assert.deepEqual(tags, [
      sibling,
      {
        name: `PR: ${state[0]}${state.slice(1).toLowerCase()}`,
        dot: { DRAFT: '#9198A1', OPEN: '#3FB950', CLOSED: '#F85149', MERGED: '#A371F7' }[state],
      },
    ]);
    const previous = writes;
    await updatePullRequestTag(service, state);
    assert.equal(writes, previous);
  }
  await assert.rejects(updatePullRequestTag(service, 'unexpected'), /Unknown PR state/);
  assert.equal(tags.length, 2);
  await updatePullRequestTag(service, null);
  assert.deepEqual(tags, [sibling]);
});

// Only SDK I/O is replaced; authentication and GraphQL pagination use production code.
const { createAuthController } = await import('../src/plugins/pull-request/auth.ts');
const { fetchPullRequest, GitHubError } = await import('../src/plugins/pull-request/github.ts');
const response = (data, status = 200, headers = {}) => ({
  status,
  headers,
  body: JSON.stringify(data),
});
function authFixture(saved = null) {
  const store = { value: saved, loads: 0 };
  const sdk = {
    env: {
      async get() {
        return null;
      },
    },
    credentials: {
      async get() {
        ++store.loads;
        return store.value;
      },
      async set(key, value) {
        store.value = value;
      },
      async delete() {
        store.value = null;
      },
    },
    http: {
      async request({ headers }) {
        if (headers.Authorization === 'Bearer invalid') return response({}, 401);
        return response({ data: { viewer: { login: headers.Authorization.slice(7) } } });
      },
    },
  };
  return { sdk, store };
}
const tick = () => new Promise((resolve) => setImmediate(resolve));
test('one account across attachments; replacement failure, restart and final release', async () => {
  const auth = createAuthController();
  const { sdk, store } = authFixture();
  const stopA = auth.subscribe(sdk, () => {});
  const stopB = auth.subscribe(sdk, () => {});
  await tick();
  assert.equal(store.loads, 1);
  await auth.connect(sdk, 'alice');
  assert.equal(auth.snapshot().login, 'alice');
  const saved = store.value;
  await auth.connect(sdk, 'invalid');
  assert.equal(auth.snapshot().login, 'alice');
  assert.equal(store.value, saved);
  assert.match(auth.snapshot().error, /invalid/);
  stopA();
  assert.equal(await auth.run(sdk, async (_, login) => login), 'alice');
  stopB();
  assert.equal(auth.snapshot().login, undefined);
  const restarted = createAuthController();
  const stop = restarted.subscribe(sdk, () => {});
  await tick();
  assert.equal(restarted.snapshot().login, 'alice');
  await restarted.connect(sdk, 'bob');
  assert.equal(restarted.snapshot().login, 'bob');
  await restarted.disconnect(sdk);
  assert.equal(store.value, null);
  stop();
});
test('disconnect and replacement abort work and reject late responses from old account', async () => {
  for (const action of ['disconnect', 'replace']) {
    const auth = createAuthController();
    const { sdk } = authFixture(JSON.stringify({ token: 'alice', login: 'alice' }));
    const stop = auth.subscribe(sdk, () => {});
    await tick();
    let release, signal;
    const work = auth.run(sdk, async (_, __, s) => {
      signal = s;
      return new Promise((resolve) => {
        release = resolve;
      });
    });
    const rejected = assert.rejects(work, /cancelled/);
    if (action === 'disconnect') await auth.disconnect(sdk);
    else await auth.connect(sdk, 'bob');
    assert.equal(signal.aborted, true);
    release('stale PR');
    await rejected;
    stop();
  }
});
test('revoked tokens pause polling; permission errors retain connection; rate limits defer retries', async () => {
  const auth = createAuthController();
  const { sdk } = authFixture(JSON.stringify({ token: 'alice', login: 'alice' }));
  const stop = auth.subscribe(sdk, () => {});
  await tick();
  await assert.rejects(
    auth.run(sdk, async () => {
      throw new GitHubError('permission', 'access denied');
    }),
    /access denied/,
  );
  assert.equal(auth.snapshot().paused, false);
  await assert.rejects(
    auth.run(sdk, async () => {
      throw new GitHubError('rate', 'limited', Date.now() + 120_000);
    }),
    /limited/,
  );
  await assert.rejects(
    auth.run(sdk, async () => 'unexpected'),
    /rate limit/,
  );
  await auth.connect(sdk, 'bob');
  await assert.rejects(
    auth.run(sdk, async () => {
      throw new GitHubError('authentication', 'revoked');
    }),
    /revoked/,
  );
  assert.equal(auth.snapshot().paused, true);
  await assert.rejects(
    auth.run(sdk, async () => 'unexpected'),
    /valid GitHub token/,
  );
  assert.equal(auth.snapshot().login, 'bob');
  stop();
});
const connection = (nodes, cursor = null) => ({
  nodes,
  pageInfo: { hasNextPage: cursor !== null, endCursor: cursor },
});
function prPage(reviews, checks) {
  return {
    data: {
      repository: {
        pullRequest: {
          id: 'PR_node',
          number: 42,
          title: 'Paginated PR',
          state: 'MERGED',
          url: 'https://github.com/example/repo/pull/42',
          reviewDecision: 'APPROVED',
          reviews,
          commits: {
            nodes: [{ commit: { id: 'commit_node', statusCheckRollup: { contexts: checks } } }],
          },
        },
      },
    },
  };
}
test('reviews and latest commit checks paginate independently; GraphQL IDs and merged state survive', async () => {
  const requests = [];
  const sdk = {
    http: {
      async request(request) {
        const { variables } = JSON.parse(request.body);
        requests.push(variables);
        if (!variables.id)
          return response(
            prPage(
              connection([review('graph_old', 'alice', '2026-10-25T10:00:00Z')], 'review_cursor'),
              connection([{ name: 'build', conclusion: 'SUCCESS' }], 'check_cursor'),
            ),
          );
        if (variables.id === 'PR_node')
          return response({
            data: {
              node: { reviews: connection([review('graph_new', 'bob', '2026-10-26T10:00:00Z')]) },
            },
          });
        return response({
          data: {
            node: {
              statusCheckRollup: {
                contexts: connection([{ context: 'legacy', state: 'SUCCESS' }]),
              },
            },
          },
        });
      },
    },
  };
  const result = await fetchPullRequest(sdk, { repository: 'example/repo', number: 42 }, 'dummy');
  assert.deepEqual(
    result.reviews.map((r) => r.id),
    ['graph_old', 'graph_new'],
  );
  assert.equal(result.state, 'MERGED');
  assert.deepEqual(result.statusCheckRollup, [
    { name: 'build', conclusion: 'SUCCESS' },
    { context: 'legacy', state: 'SUCCESS' },
  ]);
  assert.deepEqual(requests.slice(1), [
    { id: 'PR_node', cursor: 'review_cursor' },
    { id: 'commit_node', cursor: 'check_cursor' },
  ]);
});
test('overflow and errors on later pages reject the entire snapshot', async () => {
  for (const overflow of [true, false]) {
    const sdk = {
      http: {
        async request(request) {
          const { variables } = JSON.parse(request.body);
          if (!variables.id)
            return response(
              prPage(
                connection(
                  overflow ? Array(10_000).fill(review('id', 'alice', '2026-10-25T10:00:00Z')) : [],
                  'next',
                ),
                connection([]),
              ),
            );
          throw new Error('network offline');
        },
      },
    };
    await assert.rejects(
      fetchPullRequest(sdk, { repository: 'example/repo', number: 42 }, 'dummy'),
      overflow ? /10,000/ : /network offline/,
    );
  }
});

test('closing one attachment aborts its requests while another retains the account', async () => {
  const auth = createAuthController();
  const { sdk } = authFixture(JSON.stringify({ token: 'alice', login: 'alice' }));
  const otherSdk = { ...sdk };
  const stopA = auth.subscribe(sdk, () => {});
  const stopB = auth.subscribe(otherSdk, () => {});
  await tick();
  let release, signal;
  const work = auth.run(sdk, async (_, __, s) => {
    signal = s;
    return new Promise((resolve) => {
      release = resolve;
    });
  });
  const rejected = assert.rejects(work, /cancelled/);
  stopA();
  assert.equal(signal.aborted, true);
  assert.equal(await auth.run(otherSdk, async (_, login) => login), 'alice');
  release('stale');
  await rejected;
  stopB();
});
test('storage failure during replacement preserves saved and in-memory credentials', async () => {
  const auth = createAuthController();
  const { sdk, store } = authFixture(JSON.stringify({ token: 'alice', login: 'alice' }));
  const stop = auth.subscribe(sdk, () => {});
  await tick();
  sdk.credentials.set = async () => {
    throw new Error('Credential storage unavailable or locked');
  };
  await auth.connect(sdk, 'bob');
  assert.equal(auth.snapshot().login, 'alice');
  assert.equal(JSON.parse(store.value).token, 'alice');
  assert.match(auth.snapshot().error, /locked/);
  stop();
});
test('GitHub status and GraphQL rate limits honor reset headers; forbidden partial data is rejected', async () => {
  const { graphql } = await import('../src/plugins/pull-request/github.ts');
  const reset = Math.ceil(Date.now() / 1000) + 600;
  for (const result of [
    response({}, 429, { 'retry-after': '120' }),
    response({ errors: [{ type: 'RATE_LIMITED' }] }, 200, { 'x-ratelimit-reset': String(reset) }),
  ]) {
    const sdk = {
      http: {
        async request() {
          return result;
        },
      },
    };
    await assert.rejects(
      graphql(sdk, 'dummy', 'query { viewer { login } }'),
      (error) => error.kind === 'rate' && error.retryAt >= Date.now() + 119_000,
    );
  }
  const sdk = {
    http: {
      async request() {
        return response({ data: { repository: null }, errors: [{ type: 'FORBIDDEN' }] });
      },
    },
  };
  await assert.rejects(
    graphql(sdk, 'dummy', 'query { viewer { login } }'),
    (error) => error.kind === 'permission',
  );
});

test('successful response at zero remaining publishes data and defers following requests', async () => {
  const auth = createAuthController();
  const { sdk } = authFixture(JSON.stringify({ token: 'alice', login: 'alice' }));
  const stop = auth.subscribe(sdk, () => {});
  await tick();
  const reset = Math.ceil(Date.now() / 1000) + 600;
  sdk.http.request = async () =>
    response({ data: { viewer: { login: 'alice' } } }, 200, {
      'x-ratelimit-remaining': '0',
      'x-ratelimit-reset': String(reset),
    });
  const result = await auth.run(sdk, async (_, __, signal, transport) =>
    transport.http.request({ url: 'https://api.github.com/graphql', method: 'POST', signal }),
  );
  assert.equal(result.status, 200);
  assert.equal(auth.snapshot().retryAt, reset * 1000);
  await assert.rejects(
    auth.run(sdk, async () => 'unexpected'),
    /rate limit/,
  );
  stop();
});

test('environment precedence bypasses Secret Service and never persists the token', async () => {
  for (const [values, expected, queried] of [
    [{ GH_TOKEN: '  primary  ', GITHUB_TOKEN: 'secondary' }, 'primary', ['GH_TOKEN']],
    [{ GH_TOKEN: '  ', GITHUB_TOKEN: 'secondary' }, 'secondary', ['GH_TOKEN', 'GITHUB_TOKEN']],
  ]) {
    const auth = createAuthController();
    const { sdk, store } = authFixture(JSON.stringify({ token: 'saved', login: 'saved' }));
    const names = [];
    sdk.env.get = async (name) => {
      names.push(name);
      return values[name] ?? null;
    };
    sdk.credentials.get = async () => {
      throw new Error('Keyring unavailable; environment must work independently');
    };
    sdk.credentials.set = sdk.credentials.delete = async () => {
      throw new Error('Environment tokens must not modify saved credentials');
    };
    const stop = auth.subscribe(sdk, () => {});
    await tick();
    assert.equal(auth.snapshot().login, expected);
    assert.deepEqual(names, queried);
    assert.equal(JSON.parse(store.value).token, 'saved');
    await auth.disconnect(sdk);
    assert.equal(auth.snapshot().login, undefined);
    assert.equal(auth.snapshot().environment, queried.at(-1));
    await auth.reconnect(sdk);
    assert.equal(auth.snapshot().login, expected);
    stop();
    assert.equal(auth.snapshot().environment, undefined);
  }
});
test('unset environment falls back to saved credentials; invalid environment never silently falls back', async () => {
  const auth = createAuthController();
  const { sdk, store } = authFixture(JSON.stringify({ token: 'saved', login: 'saved' }));
  sdk.env.get = async (name) => (name === 'GH_TOKEN' ? 'invalid' : 'secondary');
  let stop = auth.subscribe(sdk, () => {});
  await tick();
  assert.equal(auth.snapshot().login, undefined);
  assert.equal(auth.snapshot().environment, 'GH_TOKEN');
  assert.match(auth.snapshot().error, /invalid/);
  assert.equal(store.loads, 0);
  assert.equal(JSON.parse(store.value).token, 'saved');
  await auth.connect(sdk, 'manual');
  assert.equal(JSON.parse(store.value).token, 'saved');
  stop();
  sdk.env.get = async () => null;
  stop = auth.subscribe(sdk, () => {});
  await tick();
  assert.equal(auth.snapshot().login, 'saved');
  assert.equal(auth.snapshot().environment, undefined);
  stop();
});
test('environment validation cannot restore credentials after the last tab closes', async () => {
  const auth = createAuthController();
  const { sdk, store } = authFixture();
  sdk.env.get = async () => 'environment';
  let release;
  sdk.http.request = async () =>
    new Promise((resolve) => {
      release = resolve;
    });
  const stop = auth.subscribe(sdk, () => {});
  await tick();
  stop();
  release(response({ data: { viewer: { login: 'stale' } } }));
  await tick();
  assert.equal(auth.snapshot().login, undefined);
  assert.equal(store.value, null);
});

test('draft status applies only to open PRs', () => {
  assert.equal(pullRequestStatus({ state: 'OPEN', isDraft: true }), 'DRAFT');
  assert.equal(pullRequestStatus({ state: 'OPEN', isDraft: false }), 'OPEN');
  assert.equal(pullRequestStatus({ state: 'CLOSED', isDraft: true }), 'CLOSED');
  assert.equal(pullRequestStatus({ state: 'MERGED', isDraft: true }), 'MERGED');
});
