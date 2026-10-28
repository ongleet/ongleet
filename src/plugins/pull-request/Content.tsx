import { useEffect, useRef, useState } from 'react';
import { useSdk } from '../../sdk';
import { parseAssociation, fetchPullRequest, type PullRequest } from './github';
import { githubAuth } from './auth';
import {
  pullRequestStatus,
  updatePullRequestTag,
  pipelineStatus,
  updatePipelineTag,
} from './status-tag';
import { refreshSeconds, DEFAULT_REFRESH_SECONDS } from './settings';
import {
  otherReviews,
  unreadReviews,
  acknowledgeReviews,
  type Review,
  type ReviewCursor,
} from './reviews';
import type { PluginContentProps } from '../types';
import type { ConfigurationEditorProps } from '..';
export function PullRequestConfigurationEditor({ value, onChange }: ConfigurationEditorProps) {
  let config: { refreshIntervalSeconds?: number } = {};
  try {
    config = JSON.parse(value);
  } catch {
    /* The YAML editor reports malformed configuration. */
  }
  return (
    <>
      <p className="muted">
        The GitHub PR association is shared by the session. Set its repository and PR number or URL
        inside the tab.
      </p>
      <label>
        Refresh interval (seconds)
        <input
          type="number"
          min="30"
          max="86400"
          step="1"
          value={config.refreshIntervalSeconds ?? DEFAULT_REFRESH_SECONDS}
          onChange={(event) =>
            onChange(
              JSON.stringify({ ...config, refreshIntervalSeconds: Number(event.target.value) }),
            )
          }
        />
      </label>
    </>
  );
}
export function PullRequestContent({
  tab,
  session,
  visible,
  configurationOpen = false,
  dismissConfiguration,
}: PluginContentProps) {
  const sdk = useSdk();
  const [auth, setAuth] = useState(githubAuth.snapshot);
  const [token, setToken] = useState('');
  const [replacing, setReplacing] = useState(false);
  const associationKey = JSON.stringify(session.pullRequest);
  const [target, setTarget] = useState(session.pullRequest?.repository ?? '');
  const [number, setNumber] = useState(String(session.pullRequest?.number ?? ''));
  const [data, setData] = useState<PullRequest>();
  const [lastRefresh, setLastRefresh] = useState<string>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const configurationDialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    if (configurationOpen) configurationDialog.current?.showModal();
    else configurationDialog.current?.close();
  }, [configurationOpen]);
  const [unread, setUnread] = useState<Review[]>([]);
  const reviewSnapshot = useRef<{ key: string; reviews: Review[] } | undefined>(undefined);
  let interval = DEFAULT_REFRESH_SECONDS;
  let configurationError = '';
  try {
    interval = refreshSeconds(tab.config);
  } catch (error) {
    configurationError = String(error);
  }
  const [intervalDraft, setIntervalDraft] = useState(String(interval));
  useEffect(() => setIntervalDraft(String(interval)), [interval]);
  const pending = useRef(false);
  const generation = useRef(0);
  const refreshRef = useRef<() => void>(() => {});
  async function syncReviewTag(hasUnread: boolean, current: number) {
    const tags = await sdk.session.listTags();
    if (current !== generation.current) return;
    const snapshot = reviewSnapshot.current;
    if (snapshot) {
      const cursor = readCursor(snapshot.key);
      if (cursor) {
        const latestUnread = unreadReviews(snapshot.reviews, cursor);
        setUnread(latestUnread);
        hasUnread = latestUnread.length > 0;
      }
    }
    const existing = tags.some((tag) => tag.name.toLowerCase() === 'pr: new review');
    if (hasUnread && !existing)
      await sdk.session.addTag({ name: 'PR: New review', dot: '#D29922', priority: 5 });
    else if (!hasUnread && existing) await sdk.session.removeTag('PR: New review');
  }
  function readCursor(key: string): ReviewCursor | null {
    const saved = localStorage.getItem(key);
    if (!saved) return null;
    const cursor = JSON.parse(saved) as ReviewCursor;
    if (
      typeof cursor.through !== 'string' ||
      !Array.isArray(cursor.ids) ||
      !cursor.ids.every((id) => typeof id === 'string')
    )
      throw new Error('Invalid saved review read state');
    return cursor;
  }
  async function markReviewsRead() {
    const snapshot = reviewSnapshot.current;
    if (!snapshot) return;
    try {
      localStorage.setItem(
        snapshot.key,
        JSON.stringify(acknowledgeReviews(snapshot.reviews, readCursor(snapshot.key) ?? undefined)),
      );
      setUnread([]);
      await syncReviewTag(false, generation.current);
    } catch (error) {
      setError(String(error));
    }
  }
  async function refresh() {
    const account = githubAuth.snapshot();
    if (
      pending.current ||
      !session.pullRequest ||
      !account.login ||
      account.paused ||
      account.loading ||
      account.busy
    )
      return;
    const request = generation.current;
    pending.current = true;
    setBusy(true);
    setError('');
    try {
      const { result, viewer } = await githubAuth.run(
        sdk,
        async (token, viewer, signal, transport) => ({
          result: await fetchPullRequest(transport, session.pullRequest!, token, signal),
          viewer,
        }),
      );
      if (request === generation.current) {
        setData(result);
        setLastRefresh(new Date().toLocaleString());
        await updatePullRequestTag(
          sdk.session,
          pullRequestStatus(result),
          () => request === generation.current,
        );
        await updatePipelineTag(
          sdk.session,
          pipelineStatus(result.statusCheckRollup) === 'Failed',
          () => request === generation.current,
        );
        if (request !== generation.current) return;
        const reviews = otherReviews(result.reviews ?? [], viewer);
        const key = `ongleet.pr-reviews.v1:${session.id}:${associationKey}:${viewer.toLowerCase()}`;
        let cursor = readCursor(key);
        if (!cursor) {
          cursor = acknowledgeReviews(reviews);
          localStorage.setItem(key, JSON.stringify(cursor));
        }
        const nextUnread = unreadReviews(reviews, cursor);
        reviewSnapshot.current = { key, reviews };
        setUnread(nextUnread);
        await syncReviewTag(nextUnread.length > 0, request);
      }
    } catch (error) {
      if (request === generation.current) setError(String(error));
    } finally {
      if (request === generation.current) {
        pending.current = false;
        setBusy(false);
      }
    }
  }
  refreshRef.current = () => {
    void refresh();
  };
  useEffect(() => {
    let version = -1;
    return githubAuth.subscribe(sdk, () => {
      const next = githubAuth.snapshot();
      setAuth(next);
      if (version !== next.version) {
        version = next.version;
        ++generation.current;
        pending.current = false;
        setBusy(false);
        setData(undefined);
        setLastRefresh(undefined);
        setUnread([]);
        reviewSnapshot.current = undefined;
        setError('');
        setToken('');
        setReplacing(false);
        void sdk.session.removeTag('PR: New review').catch((error) => setError(String(error)));
      }
      if (!next.loading && !next.busy && next.login && !next.paused) refreshRef.current();
    });
  }, [sdk]);
  useEffect(() => {
    setTarget(session.pullRequest?.repository ?? '');
    setNumber(String(session.pullRequest?.number ?? ''));
    setData(undefined);
    setUnread([]);
    reviewSnapshot.current = undefined;
    setLastRefresh(undefined);
    setError('');
    pending.current = false;
    setBusy(false);
    ++generation.current;
    const current = generation.current;
    if (session.pullRequest) refreshRef.current();
    else
      void updatePullRequestTag(sdk.session, null, () => current === generation.current)
        .then(() => updatePipelineTag(sdk.session, false, () => current === generation.current))
        .then(() => syncReviewTag(false, current))
        .catch((error) => {
          if (current === generation.current) setError(String(error));
        });
    // Namespace replacement and host disposal cancel native work.
    return () => {
      ++generation.current;
    };
  }, [tab.id, associationKey]);
  useEffect(() => {
    if (configurationError) return;
    const timer = setInterval(() => refreshRef.current(), interval * 1000);
    return () => clearInterval(timer);
  }, [interval, configurationError]);
  async function saveConfiguration() {
    if (saving) return;
    setSaving(true);
    setError('');
    try {
      const association = parseAssociation(target, number ? Number(number) : undefined);
      const seconds = refreshSeconds({ refreshIntervalSeconds: Number(intervalDraft) });
      const current = await sdk.storage.read('session');
      await sdk.storage.configure({ refreshIntervalSeconds: seconds });
      await sdk.storage.replace('session', current?.revision ?? 0, 1, association);
      dismissConfiguration?.();
    } catch (error) {
      setError(String(error));
    } finally {
      setSaving(false);
    }
  }
  const configuration = (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        void saveConfiguration();
      }}
      className="pr-configuration-fields"
    >
      <div className="pr-association-fields">
        <label>
          Repository or PR URL
          <input
            placeholder="owner/repository or GitHub PR URL"
            value={target}
            onChange={(event) => setTarget(event.target.value)}
          />
        </label>
        <label>
          PR number
          <input
            type="number"
            min="1"
            placeholder="PR number"
            value={number}
            onChange={(event) => setNumber(event.target.value)}
          />
        </label>
      </div>
      <p className="muted">
        This association belongs to the session. Clear the repository to remove it. Status and
        session tags refresh every {interval} seconds while this tab is open, including when hidden.
      </p>
      <label>
        Refresh interval (seconds)
        <input
          type="number"
          min="30"
          max="86400"
          step="1"
          value={intervalDraft}
          onChange={(event) => setIntervalDraft(event.target.value)}
        />
      </label>
      <div className="dialog-actions">
        {configurationOpen && (
          <button type="button" disabled={saving} onClick={dismissConfiguration}>
            Cancel
          </button>
        )}
        <button disabled={saving}>{saving ? 'Saving…' : 'Save'}</button>
      </div>
    </form>
  );
  const connection = (
    <section aria-label="GitHub connection">
      <h3>GitHub connection</h3>
      {!auth.loading && !auth.environment && (
        <p className="muted">
          Neither GH_TOKEN nor GITHUB_TOKEN is set in Ongleet’s launch environment.
        </p>
      )}
      {auth.login && <p>Connected as {auth.login}</p>}
      {auth.environment ? (
        <>
          <p className="muted">
            Using {auth.environment} from Ongleet’s launch environment. Update it and restart
            Ongleet to change the token.
          </p>
          {(!auth.login || auth.paused) && (
            <button
              disabled={auth.busy || auth.loading}
              onClick={() => void githubAuth.reconnect(sdk)}
            >
              Reconnect
            </button>
          )}
        </>
      ) : auth.login && !replacing && !auth.paused ? (
        <button onClick={() => setReplacing(true)} disabled={auth.busy}>
          Replace token
        </button>
      ) : (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            const value = token;
            setToken('');
            void githubAuth.connect(sdk, value).catch((error) => setError(String(error)));
          }}
        >
          <label>
            GitHub personal access token
            <input
              type="password"
              autoComplete="off"
              value={token}
              onChange={(event) => setToken(event.target.value)}
              disabled={auth.busy || auth.loading}
            />
          </label>
          <button disabled={auth.busy || auth.loading || !token.trim()}>
            {auth.login ? 'Replace token' : 'Connect'}
          </button>
        </form>
      )}
      {auth.login && (
        <button
          disabled={auth.busy || auth.loading}
          onClick={() => void githubAuth.disconnect(sdk)}
        >
          Disconnect
        </button>
      )}
      <button
        onClick={() =>
          void sdk.desktop
            .openUrl('https://github.com/settings/tokens')
            .catch((error) => setError(String(error)))
        }
      >
        GitHub token settings
      </button>
      {auth.loading && <p>Loading GitHub connection…</p>}
      {auth.error && (
        <p role="alert" className="error">
          {auth.error}
        </p>
      )}
    </section>
  );
  return (
    <section
      role="tabpanel"
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
      hidden={!visible}
      className="plugin-panel pr-panel"
    >
      <div className="pr-content">
        {!auth.loading && (!auth.login || auth.paused) && (
          <p className="muted">Configure this tab to connect your GitHub account.</p>
        )}
        {!session.pullRequest && !configurationOpen && configuration}
        <dialog
          ref={configurationDialog}
          aria-label="Pull request configuration"
          onCancel={(event) => {
            if (saving) event.preventDefault();
            else dismissConfiguration?.();
          }}
        >
          {configurationOpen && (
            <div className="pr-configuration-body">
              <h2>Configure {tab.name}</h2>
              {connection}
              {configuration}
              {error && (
                <p role="alert" className="error">
                  {error}
                </p>
              )}
            </div>
          )}
        </dialog>
        {session.pullRequest && (
          <header className="pr-heading">
            <h2>
              {data
                ? `#${data.number} ${data.title}`
                : `${session.pullRequest.repository} #${session.pullRequest.number}`}{' '}
              <button
                type="button"
                className="pr-refresh"
                aria-label={busy ? 'Refreshing pull request' : 'Refresh pull request'}
                title={busy ? 'Refreshing…' : 'Refresh pull request'}
                disabled={busy || !auth.login || auth.paused || auth.busy || auth.loading}
                onClick={() => void refresh()}
              >
                <svg
                  width="16"
                  height="16"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden="true"
                >
                  <path d="M20 7v5h-5M4 17v-5h5" />
                  <path d="M6 7a7 7 0 0 1 12-1l2 6M4 12l2 6a7 7 0 0 0 12-1" />
                </svg>
              </button>
            </h2>
          </header>
        )}
        {configurationError && (
          <p role="alert" className="error">
            {configurationError}
          </p>
        )}
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        {unread.length > 0 && (
          <div role="status">
            <p>
              New reviews from{' '}
              {Array.from(new Set(unread.map((review) => review.author!.login))).join(', ')}
            </p>
            <ul>
              {unread.map((review) => (
                <li key={review.id}>
                  {review.author!.login}: {review.state} ·{' '}
                  {new Date(review.submittedAt!).toLocaleString()}
                </li>
              ))}
            </ul>
            <button onClick={() => void markReviewsRead()}>Mark reviews read</button>
          </div>
        )}
        {data && (
          <>
            <p>
              State: {pullRequestStatus(data)} · Review:{' '}
              {data.reviewDecision || 'No review decision'}
            </p>
            <button
              onClick={() =>
                void sdk.desktop.openUrl(data.url).catch((error) => setError(String(error)))
              }
            >
              Open pull request
            </button>
            <section aria-label="Linked issues">
              <h3>Linked issues</h3>
              {data.issues.length ? (
                <ul>
                  {data.issues.map((issue) => (
                    <li key={issue.id}>
                      <button
                        onClick={() =>
                          void sdk.desktop
                            .openUrl(issue.url)
                            .catch((error) => setError(String(error)))
                        }
                      >
                        {issue.repository.nameWithOwner}#{issue.number}: {issue.title}
                      </button>{' '}
                      · {issue.state}
                    </li>
                  ))}
                </ul>
              ) : (
                <p>No linked issues.</p>
              )}
            </section>
            <p role="status">Pipeline: {pipelineStatus(data.statusCheckRollup)}</p>
            <section aria-label="Reviews">
              <h3>Reviews</h3>
              {data.reviews?.length ? (
                <ul>
                  {data.reviews.map((review) => (
                    <li key={review.id}>
                      {review.author?.login ?? 'Deleted user'}: {review.state}
                      {review.submittedAt && Number.isFinite(Date.parse(review.submittedAt)) && (
                        <>
                          {' '}
                          ·{' '}
                          <time dateTime={review.submittedAt}>
                            {new Date(review.submittedAt).toLocaleString()}
                          </time>
                        </>
                      )}
                    </li>
                  ))}
                </ul>
              ) : (
                <p>No reviews submitted.</p>
              )}
            </section>
            <details className="pr-checks" key={associationKey}>
              <summary>Checks</summary>
              {data.statusCheckRollup?.length ? (
                <ul>
                  {data.statusCheckRollup.map((check, index) => (
                    <li key={index}>
                      {check.name ?? check.context}:{' '}
                      {check.conclusion || check.state || check.status || 'Unknown'}
                    </li>
                  ))}
                </ul>
              ) : (
                <p>No checks reported.</p>
              )}
            </details>
          </>
        )}
      </div>
      {session.pullRequest && (
        <footer className="plugin-statusbar" aria-label="Pull request status">
          <span>
            {busy
              ? 'Refreshing…'
              : error
                ? 'Refresh failed'
                : lastRefresh
                  ? `Last refresh: ${lastRefresh}`
                  : 'Not refreshed yet'}
          </span>
          <span>
            {!auth.login || auth.paused
              ? 'Connect GitHub to refresh'
              : `Auto-refresh every ${interval}s · Also refreshes when hidden`}
          </span>
        </footer>
      )}
    </section>
  );
}
