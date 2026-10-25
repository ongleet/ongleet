import { useEffect, useRef, useState } from 'react';
import { useSdk } from '../../sdk';
import { parseAssociation, fetchPullRequest, fetchViewer } from './github';
import { updatePullRequestTag } from './status-tag';
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
type PullRequest = {
  title: string;
  url: string;
  number: number;
  state: string;
  reviewDecision: string;
  reviews: Review[];
  statusCheckRollup: {
    name?: string;
    context?: string;
    status?: string;
    conclusion?: string;
    state?: string;
  }[];
};
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
export function PullRequestContent({ tab, session, visible }: PluginContentProps) {
  const sdk = useSdk();
  const associationKey = JSON.stringify(session.pullRequest);
  const [target, setTarget] = useState(session.pullRequest?.repository ?? '');
  const [number, setNumber] = useState(String(session.pullRequest?.number ?? ''));
  const [data, setData] = useState<PullRequest>();
  const [lastRefresh, setLastRefresh] = useState<string>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
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
    if (pending.current || !session.pullRequest) return;
    const request = generation.current;
    pending.current = true;
    setBusy(true);
    setError('');
    try {
      const result = (await fetchPullRequest(sdk, session.pullRequest)) as PullRequest;
      if (request === generation.current) {
        setData(result);
        setLastRefresh(new Date().toLocaleString());
        await updatePullRequestTag(sdk.session, result.state, () => request === generation.current);
        if (request !== generation.current) return;
        const viewer = await fetchViewer(sdk);
        if (request !== generation.current) return;
        const reviews = otherReviews(result.reviews ?? [], viewer);
        const key = `devcenter.pr-reviews.v1:${session.id}:${associationKey}:${viewer.toLowerCase()}`;
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
  async function associate() {
    setError('');
    try {
      const association = parseAssociation(target, number ? Number(number) : undefined);
      const current = await sdk.storage.read('session');
      await sdk.storage.replace('session', current?.revision ?? 0, 1, association);
    } catch (error) {
      setError(String(error));
    }
  }
  return (
    <section
      role="tabpanel"
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
      hidden={!visible}
      className="plugin-panel"
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void associate();
        }}
        className="plugin-toolbar"
      >
        <input
          aria-label="GitHub repository or PR URL"
          placeholder="owner/repository or GitHub PR URL"
          value={target}
          onChange={(event) => setTarget(event.target.value)}
        />
        <input
          aria-label="PR number"
          type="number"
          min="1"
          placeholder="PR number"
          value={number}
          onChange={(event) => setNumber(event.target.value)}
        />
        <button>Save association</button>
      </form>
      <p className="muted">
        This association belongs to the session. Clear the repository to remove it. Status and
        session tags refresh every {interval} seconds while this tab is open, including when hidden.
      </p>
      <form
        className="plugin-toolbar"
        onSubmit={(event) => {
          event.preventDefault();
          try {
            const seconds = refreshSeconds({ refreshIntervalSeconds: Number(intervalDraft) });
            void sdk.storage
              .configure({ refreshIntervalSeconds: seconds })
              .catch((error) => setError(String(error)));
          } catch (error) {
            setError(String(error));
          }
        }}
      >
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
        <button>Save interval</button>
      </form>
      {configurationError && (
        <p role="alert" className="error">
          {configurationError}
        </p>
      )}
      <button disabled={busy || !session.pullRequest} onClick={() => void refresh()}>
        {busy ? 'Refreshing…' : 'Refresh'}
      </button>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {lastRefresh && <p className="muted">Last successful refresh: {lastRefresh}</p>}
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
          <h2>
            #{data.number} {data.title}
          </h2>
          <p>
            State: {data.state} · Review: {data.reviewDecision || 'No review decision'}
          </p>
          <button
            onClick={() =>
              void sdk.desktop.openUrl(data.url).catch((error) => setError(String(error)))
            }
          >
            Open pull request
          </button>
          <h3>Checks</h3>
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
        </>
      )}
    </section>
  );
}
