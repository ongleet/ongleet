import { t, getLocale } from '../../i18n';
import {
  Button,
  Dialog,
  Disclosure,
  DisclosureTrigger,
  Input,
  Select,
  TabPanel,
} from '../../components/Controls';
import { useEffect, useRef, useState } from 'react';
import { useSdk, useTabInteractions, useSessionSettings } from '../../sdk';
import { parseAssociation, fetchPullRequest, type PullRequest } from './github';
import { githubAuth, sessionAuth, tabAuth } from './auth';
import { GitHubConnection } from './Connection';
import { prSessionDefaults } from './SessionConfiguration';
import {
  pullRequestStatus,
  updatePullRequestTag,
  pipelineStatus,
  updatePipelineTag,
} from './status-tag';
import { Checkbox } from '../../components/Checkbox';
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
  let config: { refreshIntervalSeconds?: number | null; connection?: string } = {};
  try {
    config = JSON.parse(value);
  } catch {
    /* The YAML editor reports malformed configuration. */
  }
  return (
    <>
      <p className="muted">
        {t(
          'The GitHub PR association is shared by the session. Set its repository and PR number or URL inside the tab.',
        )}
      </p>
      <label className="checkbox-field">
        <Checkbox
          checked={config.refreshIntervalSeconds != null}
          onChange={(event) =>
            onChange(
              JSON.stringify({
                ...config,
                refreshIntervalSeconds: event.target.checked ? DEFAULT_REFRESH_SECONDS : null,
              }),
            )
          }
        />
        {t('Override refresh interval for this tab')}
      </label>
      <label>
        {t('Refresh interval (seconds)')}
        <Input
          disabled={config.refreshIntervalSeconds == null}
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
  openSessionConfiguration,
  openConfiguration,
}: PluginContentProps) {
  const sdk = useSdk();
  const shared = useSessionSettings(sdk, prSessionDefaults);
  const localConnection = (tab.config as { connection?: string } | null)?.connection === 'tab';
  const controller = localConnection
    ? tabAuth(tab.id)
    : shared.value.connection === 'session'
      ? sessionAuth(session.id)
      : githubAuth;
  const [auth, setAuth] = useState(controller.snapshot);
  const localAssociation = (
    tab.config as { pullRequest?: { repository: string; number: number } | null } | null
  )?.pullRequest;
  const association = localAssociation ?? session.pullRequest;
  const [associationLocalDraft, setAssociationLocalDraft] = useState(!!localAssociation);
  useEffect(() => setAssociationLocalDraft(!!localAssociation), [!!localAssociation]);
  const associationKey = JSON.stringify(association);
  const [target, setTarget] = useState(association?.repository ?? '');
  const [number, setNumber] = useState(String(association?.number ?? ''));
  const [data, setData] = useState<PullRequest>();
  const [lastRefresh, setLastRefresh] = useState<string>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  useTabInteractions({
    state: () => ({
      summary: data ? `${data.title.slice(0, 512)} (${data.state})` : 'PR unavailable',
      data: {
        loading: busy,
        url: data?.url ?? null,
        title: data?.title.slice(0, 4096) ?? null,
        state: data?.state ?? null,
        draft: data?.isDraft ?? null,
        reviewDecision: data?.reviewDecision ?? null,
        reviewCount: data?.reviews.length ?? 0,
        reviewSummary: Object.fromEntries(
          ['APPROVED', 'CHANGES_REQUESTED', 'COMMENTED', 'DISMISSED', 'PENDING'].map((state) => [
            state,
            data?.reviews.filter((review) => review.state === state).length ?? 0,
          ]),
        ),
        checkCount: data?.statusCheckRollup.length ?? 0,
        pipeline: data ? pipelineStatus(data.statusCheckRollup) : null,
        lastRefresh: lastRefresh ?? null,
        refreshError: error.slice(0, 4096) || null,
      },
      observedAt: lastRefresh ?? null,
      stale: !!error || busy || !data,
    }),
  });
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
    interval = refreshSeconds(
      (tab.config as { refreshIntervalSeconds?: number | null } | null)?.refreshIntervalSeconds ==
        null
        ? shared.value
        : tab.config,
    );
  } catch (error) {
    configurationError = String(error);
  }
  const localInterval = (tab.config as { refreshIntervalSeconds?: number | null } | null)
    ?.refreshIntervalSeconds;
  const [intervalDraft, setIntervalDraft] = useState(
    localInterval == null ? '' : String(localInterval),
  );
  useEffect(
    () => setIntervalDraft(localInterval == null ? '' : String(localInterval)),
    [localInterval],
  );
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
      throw new Error(t('Invalid saved review read state'));
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
    const account = controller.snapshot();
    if (
      (!localConnection && shared.loading) ||
      (!localConnection && !!shared.error) ||
      pending.current ||
      !association ||
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
      const { result, viewer } = await controller.run(
        sdk,
        async (token, viewer, signal, transport) => ({
          result: await fetchPullRequest(transport, association!, token, signal),
          viewer,
        }),
      );
      if (request === generation.current) {
        setData(result);
        setLastRefresh(new Date().toISOString());
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
    return controller.subscribe(sdk, () => {
      const next = controller.snapshot();
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
        void sdk.session.removeTag('PR: New review').catch((error) => setError(String(error)));
      }
      if (!next.loading && !next.busy && next.login && !next.paused) refreshRef.current();
    });
  }, [sdk, controller]);
  useEffect(() => {
    if (!shared.loading && !shared.error) refreshRef.current();
  }, [shared.loading, shared.error]);
  useEffect(() => {
    setTarget(association?.repository ?? '');
    setNumber(String(association?.number ?? ''));
    setData(undefined);
    setUnread([]);
    reviewSnapshot.current = undefined;
    setLastRefresh(undefined);
    setError('');
    pending.current = false;
    setBusy(false);
    ++generation.current;
    const current = generation.current;
    if (association) refreshRef.current();
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
      const seconds =
        intervalDraft === ''
          ? null
          : refreshSeconds({ refreshIntervalSeconds: Number(intervalDraft) });
      const current = await sdk.storage.read('session');
      await sdk.storage.configure({
        refreshIntervalSeconds: seconds,
        pullRequest: associationLocalDraft ? association : null,
      });
      if (!configurationOpen && !associationLocalDraft)
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
      <label className="checkbox-field">
        <Checkbox
          checked={associationLocalDraft}
          onChange={(event) => {
            setAssociationLocalDraft(event.target.checked);
            if (!event.target.checked) {
              setTarget(session.pullRequest?.repository ?? '');
              setNumber(String(session.pullRequest?.number ?? ''));
            }
          }}
        />
        {t('Use a different pull request for this tab')}
      </label>
      <div className="pr-association-fields">
        <label>
          {t('Repository or PR URL')}
          <Input
            disabled={configurationOpen && !associationLocalDraft}
            placeholder={t('owner/repository or GitHub PR URL')}
            value={target}
            onChange={(event) => setTarget(event.target.value)}
          />
        </label>
        <label>
          {t('PR number')}
          <Input
            type="number"
            min="1"
            disabled={configurationOpen && !associationLocalDraft}
            placeholder={t('PR number')}
            value={number}
            onChange={(event) => setNumber(event.target.value)}
          />
        </label>
      </div>
      <p className="muted">
        {t('Use the session association or override it for this tab. Status refreshes every ')}
        {interval}
        {t(' seconds while this tab is open, including when hidden.')}
      </p>
      <label className="checkbox-field">
        <Checkbox
          checked={intervalDraft !== ''}
          onChange={(event) => setIntervalDraft(event.target.checked ? String(interval) : '')}
        />
        {t('Override refresh interval for this tab')}
      </label>
      <label>
        {t('Refresh interval (seconds)')}
        <Input
          disabled={intervalDraft === ''}
          type="number"
          min="30"
          max="86400"
          step="1"
          value={intervalDraft === '' ? interval : intervalDraft}
          onChange={(event) => setIntervalDraft(event.target.value)}
        />
      </label>
      <div className="dialog-actions">
        {configurationOpen && (
          <Button type="button" disabled={saving} onClick={dismissConfiguration}>
            {t('Cancel')}
          </Button>
        )}
        <Button disabled={saving}>{saving ? t('Saving…') : t('Save')}</Button>
      </div>
    </form>
  );

  return (
    <TabPanel
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
      hidden={!visible}
      className="plugin-panel pr-panel"
    >
      <div className="pr-content">
        {shared.error && (
          <p className="error" role="alert">
            {shared.error}
          </p>
        )}
        {!auth.loading && (!auth.login || auth.paused) && (
          <Button
            type="button"
            onClick={localConnection ? openConfiguration : openSessionConfiguration}
          >
            {localConnection
              ? t('Configure this tab to connect your GitHub account.')
              : t('Open shared session settings to connect your GitHub account.')}
          </Button>
        )}
        {!association && !configurationOpen && configuration}
        <Dialog
          ref={configurationDialog}
          aria-label={t('Pull request configuration')}
          onCancel={(event) => {
            if (saving) event.preventDefault();
            else dismissConfiguration?.();
          }}
        >
          {configurationOpen && (
            <div className="pr-configuration-body">
              <h2>
                {t('Configure ')}
                {tab.name}
              </h2>
              <label>
                {t('GitHub account')}
                <Select
                  value={localConnection ? 'tab' : 'inherit'}
                  onChange={(event) => {
                    void sdk.storage
                      .configure({ connection: event.target.value })
                      .catch((error) => setError(String(error)));
                  }}
                >
                  <option value="inherit">{t('Use session settings')}</option>
                  <option value="tab">{t('Account for this tab')}</option>
                </Select>
              </label>
              {localConnection ? (
                <GitHubConnection controller={controller} />
              ) : (
                <Button
                  type="button"
                  onClick={() => {
                    dismissConfiguration?.();
                    openSessionConfiguration?.();
                  }}
                >
                  {t('Shared session settings')}
                </Button>
              )}

              {configuration}
              {error && (
                <p role="alert" className="error">
                  {error}
                </p>
              )}
            </div>
          )}
        </Dialog>
        {association && (
          <header className="pr-heading">
            <h2>
              {data
                ? `#${data.number} ${data.title}`
                : `${association.repository} #${association.number}`}{' '}
              <Button
                type="button"
                className="pr-refresh"
                aria-label={busy ? t('Refreshing pull request') : t('Refresh pull request')}
                title={busy ? t('Refreshing…') : t('Refresh pull request')}
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
              </Button>
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
              {t('New reviews from')}{' '}
              {Array.from(new Set(unread.map((review) => review.author!.login))).join(', ')}
            </p>
            <ul>
              {unread.map((review) => (
                <li key={review.id}>
                  {review.author!.login}: {review.state} ·{' '}
                  {new Date(review.submittedAt!).toLocaleString(getLocale())}
                </li>
              ))}
            </ul>
            <Button onClick={() => void markReviewsRead()}>{t('Mark reviews read')}</Button>
          </div>
        )}
        {data && (
          <>
            <p>
              {t('State: ')}
              {pullRequestStatus(data)}
              {t(' · Review:')} {data.reviewDecision || t('No review decision')}
            </p>
            <Button
              onClick={() =>
                void sdk.desktop.openUrl(data.url).catch((error) => setError(String(error)))
              }
            >
              {t('Open pull request')}
            </Button>
            <section aria-label={t('Linked issues')}>
              <h3>{t('Linked issues')}</h3>
              {data.issues.length ? (
                <ul>
                  {data.issues.map((issue) => (
                    <li key={issue.id}>
                      <Button
                        onClick={() =>
                          void sdk.desktop
                            .openUrl(issue.url)
                            .catch((error) => setError(String(error)))
                        }
                      >
                        {issue.repository.nameWithOwner}#{issue.number}: {issue.title}
                      </Button>{' '}
                      · {issue.state}
                    </li>
                  ))}
                </ul>
              ) : (
                <p>{t('No linked issues.')}</p>
              )}
            </section>
            <p role="status">
              {t('Pipeline: ')}
              {pipelineStatus(data.statusCheckRollup)}
            </p>
            <section aria-label={t('Reviews')}>
              <h3>{t('Reviews')}</h3>
              {data.reviews?.length ? (
                <ul>
                  {data.reviews.map((review) => (
                    <li key={review.id}>
                      {review.author?.login ?? t('Deleted user')}: {review.state}
                      {review.submittedAt && Number.isFinite(Date.parse(review.submittedAt)) && (
                        <>
                          {' '}
                          ·{' '}
                          <time dateTime={review.submittedAt}>
                            {new Date(review.submittedAt).toLocaleString(getLocale())}
                          </time>
                        </>
                      )}
                    </li>
                  ))}
                </ul>
              ) : (
                <p>{t('No reviews submitted.')}</p>
              )}
            </section>
            <Disclosure className="pr-checks" key={associationKey}>
              <DisclosureTrigger>{t('Checks')}</DisclosureTrigger>
              {data.statusCheckRollup?.length ? (
                <ul>
                  {data.statusCheckRollup.map((check, index) => (
                    <li key={index}>
                      {check.name ?? check.context}:{' '}
                      {check.conclusion || check.state || check.status || t('Unknown')}
                    </li>
                  ))}
                </ul>
              ) : (
                <p>{t('No checks reported.')}</p>
              )}
            </Disclosure>
          </>
        )}
      </div>
      {association && (
        <footer className="plugin-statusbar" aria-label={t('Pull request status')}>
          <span>
            {busy
              ? t('Refreshing…')
              : error
                ? t('Refresh failed')
                : lastRefresh
                  ? t('Last refresh: {value0}', {
                      value0: new Date(lastRefresh).toLocaleString(getLocale()),
                    })
                  : t('Not refreshed yet')}
          </span>
          <span>
            {!auth.login || auth.paused
              ? t('Connect GitHub to refresh')
              : t('Auto-refresh every {value0}s · Also refreshes when hidden', {
                  value0: interval,
                })}
          </span>
        </footer>
      )}
    </TabPanel>
  );
}
