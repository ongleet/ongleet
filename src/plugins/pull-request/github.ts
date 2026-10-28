import type { PluginSdk } from '../../sdk';
export type Association = { repository: string; number: number };
export function parseAssociation(target: string, number?: number): Association | null {
  target = target.trim();
  if (!target) return null;
  if (target.startsWith('https://github.com/')) {
    const parts = target.slice('https://github.com/'.length).replace(/\/+$/, '').split('/');
    if (parts.length !== 4 || parts[2] !== 'pull' || !/^\d+$/.test(parts[3]))
      throw new Error('Expected a GitHub pull request URL');
    target = `${parts[0]}/${parts[1]}`;
    number = Number(parts[3]);
  }
  if (
    target.length > 256 ||
    !/^[A-Za-z0-9_][A-Za-z0-9_.-]*\/[A-Za-z0-9_][A-Za-z0-9_.-]*$/.test(target) ||
    !Number.isSafeInteger(number) ||
    number! <= 0
  )
    throw new Error('Expected owner/repository and a positive PR number');
  return { repository: target, number: number! };
}

export class GitHubError extends Error {
  readonly kind: 'authentication' | 'permission' | 'rate' | 'network';
  readonly retryAt: number;
  constructor(kind: GitHubError['kind'], message: string, retryAt = 0) {
    super(message);
    this.kind = kind;
    this.retryAt = retryAt;
  }
}
export async function graphql<T>(
  sdk: PluginSdk,
  token: string,
  query: string,
  variables: Record<string, unknown> = {},
  signal?: AbortSignal,
): Promise<T> {
  const response = await sdk.http.request({
    url: 'https://api.github.com/graphql',
    method: 'POST',
    headers: {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
      'User-Agent': 'Ongleet',
      Accept: 'application/vnd.github+json',
    },
    body: JSON.stringify({ query, variables }),
    signal,
  });
  const retry = response.headers['retry-after'];
  const reset = Number(response.headers['x-ratelimit-reset']) * 1000;
  const retryAt = retry
    ? /^\d+$/.test(retry)
      ? Date.now() + Number(retry) * 1000
      : Date.parse(retry)
    : reset;
  if (response.status === 401)
    throw new GitHubError(
      'authentication',
      'GitHub token expired, revoked or invalid. Replace token to resume.',
    );
  if (
    response.status === 429 ||
    (response.status === 403 && (retry || response.headers['x-ratelimit-remaining'] === '0'))
  )
    throw new GitHubError(
      'rate',
      'GitHub rate limit reached; polling will resume after the retry time.',
      Math.max(Date.now() + 60_000, retryAt || 0),
    );
  if (response.status === 403)
    throw new GitHubError(
      'permission',
      'GitHub denied access. Check token repository permissions and organization policies.',
    );
  if (response.status !== 200)
    throw new GitHubError('network', `GitHub request failed (HTTP ${response.status}).`);
  const result = JSON.parse(response.body) as {
    data?: T;
    errors?: { type?: string; message?: string }[];
  };
  if (result.errors?.length) {
    if (result.errors.some((e) => e.type === 'RATE_LIMITED'))
      throw new GitHubError(
        'rate',
        'GitHub rate limit reached; polling paused until reset.',
        Math.max(Date.now() + 60_000, retryAt || 0),
      );
    if (result.errors.some((e) => e.type === 'UNAUTHORIZED'))
      throw new GitHubError('authentication', 'GitHub token is invalid. Replace token to resume.');
    throw new GitHubError(
      'permission',
      'GitHub could not read the PR, linked issues or checks. Check repository access, token permissions and organization policies.',
    );
  }
  if (!result.data) throw new Error('Invalid GitHub response');
  return result.data;
}
export async function fetchViewer(
  sdk: PluginSdk,
  token: string,
  signal?: AbortSignal,
): Promise<string> {
  const data = await graphql<{ viewer: { login: string } }>(
    sdk,
    token,
    'query { viewer { login } }',
    {},
    signal,
  );
  if (!/^[A-Za-z0-9-]+(?:\[bot\])?$/.test(data.viewer?.login))
    throw new Error('Invalid GitHub account response');
  return data.viewer.login;
}
type Connection<T> = { nodes: T[]; pageInfo: { hasNextPage: boolean; endCursor: string | null } };
export type Check = {
  name?: string;
  context?: string;
  status?: string;
  conclusion?: string;
  state?: string;
};
export type LinkedIssue = {
  id: string;
  number: number;
  title: string;
  url: string;
  state: string;
  repository: { nameWithOwner: string };
};
export type PullRequest = {
  number: number;
  title: string;
  url: string;
  state: string;
  isDraft: boolean;
  reviewDecision: string;
  reviews: import('./reviews').Review[];
  statusCheckRollup: Check[];
  issues: LinkedIssue[];
};
function append<T>(target: T[], connection: Connection<T>) {
  if (!connection || !Array.isArray(connection.nodes) || !connection.pageInfo)
    throw new Error('Invalid GitHub collection');
  target.push(...connection.nodes);
  if (target.length > 10_000 || (target.length === 10_000 && connection.pageInfo.hasNextPage))
    throw new Error('GitHub collection exceeds 10,000 entries; incomplete data was discarded.');
  if (connection.pageInfo.hasNextPage && !connection.pageInfo.endCursor)
    throw new Error('Invalid GitHub pagination cursor');
  return connection.pageInfo.hasNextPage ? connection.pageInfo.endCursor : null;
}
export async function fetchPullRequest(
  sdk: PluginSdk,
  association: Association,
  token: string,
  signal?: AbortSignal,
): Promise<PullRequest> {
  const [owner, name] = association.repository.split('/');
  type Pr = Omit<PullRequest, 'reviews' | 'statusCheckRollup' | 'issues'> & {
    id: string;
    closingIssuesReferences: Connection<LinkedIssue> | null;
    reviews: Connection<import('./reviews').Review>;
    commits: {
      nodes: {
        commit: { id: string; statusCheckRollup: { contexts: Connection<Check> } | null };
      }[];
    };
  };
  const data = await graphql<{ repository: { pullRequest: Pr } | null }>(
    sdk,
    token,
    `
      query ($owner: String!, $name: String!, $number: Int!) {
        repository(owner: $owner, name: $name) {
          pullRequest(number: $number) {
            id
            number
            title
            url
            state
            isDraft
            reviewDecision
            closingIssuesReferences(first: 100) {
              nodes {
                id
                number
                title
                url
                state
                repository {
                  nameWithOwner
                }
              }
              pageInfo {
                hasNextPage
                endCursor
              }
            }
            reviews(first: 100) {
              nodes {
                id
                author {
                  login
                }
                state
                submittedAt
              }
              pageInfo {
                hasNextPage
                endCursor
              }
            }
            commits(last: 1) {
              nodes {
                commit {
                  id
                  statusCheckRollup {
                    contexts(first: 100) {
                      nodes {
                        ... on CheckRun {
                          name
                          status
                          conclusion
                        }
                        ... on StatusContext {
                          context
                          state
                        }
                      }
                      pageInfo {
                        hasNextPage
                        endCursor
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    `,
    { owner, name, number: association.number },
    signal,
  );
  const pr = data.repository?.pullRequest;
  if (!pr)
    throw new GitHubError(
      'permission',
      'PR unavailable. Check its number and token repository access.',
    );
  const reviews: PullRequest['reviews'] = [];
  let cursor = append(reviews, pr.reviews);
  const seen = new Set<string>();
  while (cursor) {
    if (seen.has(cursor)) throw new Error('Repeated GitHub pagination cursor');
    seen.add(cursor);
    const page: { node: { reviews: Connection<import('./reviews').Review> } } = await graphql(
      sdk,
      token,
      `
        query ($id: ID!, $cursor: String!) {
          node(id: $id) {
            ... on PullRequest {
              reviews(first: 100, after: $cursor) {
                nodes {
                  id
                  author {
                    login
                  }
                  state
                  submittedAt
                }
                pageInfo {
                  hasNextPage
                  endCursor
                }
              }
            }
          }
        }
      `,
      { id: pr.id, cursor },
      signal,
    );
    cursor = append(reviews, page.node.reviews);
  }
  const checks: Check[] = [];
  const commit = pr.commits.nodes[0]?.commit;
  cursor = commit?.statusCheckRollup ? append(checks, commit.statusCheckRollup.contexts) : null;
  seen.clear();
  while (cursor) {
    if (seen.has(cursor)) throw new Error('Repeated GitHub pagination cursor');
    seen.add(cursor);
    const page: { node: { statusCheckRollup: { contexts: Connection<Check> } } } = await graphql(
      sdk,
      token,
      `
        query ($id: ID!, $cursor: String!) {
          node(id: $id) {
            ... on Commit {
              statusCheckRollup {
                contexts(first: 100, after: $cursor) {
                  nodes {
                    ... on CheckRun {
                      name
                      status
                      conclusion
                    }
                    ... on StatusContext {
                      context
                      state
                    }
                  }
                  pageInfo {
                    hasNextPage
                    endCursor
                  }
                }
              }
            }
          }
        }
      `,
      { id: commit!.id, cursor },
      signal,
    );
    cursor = append(checks, page.node.statusCheckRollup.contexts);
  }
  const issues: LinkedIssue[] = [];
  cursor = pr.closingIssuesReferences ? append(issues, pr.closingIssuesReferences) : null;
  seen.clear();
  while (cursor) {
    if (seen.has(cursor)) throw new Error('Repeated GitHub pagination cursor');
    seen.add(cursor);
    const page: { node: { closingIssuesReferences: Connection<LinkedIssue> } } = await graphql(
      sdk,
      token,
      `
        query ($id: ID!, $cursor: String!) {
          node(id: $id) {
            ... on PullRequest {
              closingIssuesReferences(first: 100, after: $cursor) {
                nodes {
                  id
                  number
                  title
                  url
                  state
                  repository {
                    nameWithOwner
                  }
                }
                pageInfo {
                  hasNextPage
                  endCursor
                }
              }
            }
          }
        }
      `,
      { id: pr.id, cursor },
      signal,
    );
    cursor = append(issues, page.node.closingIssuesReferences);
  }
  return {
    issues,
    number: pr.number,
    title: pr.title,
    url: pr.url,
    state: pr.state,
    isDraft: pr.isDraft,
    reviewDecision: pr.reviewDecision,
    reviews,
    statusCheckRollup: checks,
  };
}
