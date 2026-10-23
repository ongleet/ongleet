import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { PluginContentProps } from '../types';
import type { ConfigurationEditorProps } from '..';
type PullRequest = { title: string; url: string; number: number; state: string; reviewDecision: string; statusCheckRollup: { name?: string; context?: string; status?: string; conclusion?: string; state?: string }[] };
export function PullRequestConfigurationEditor(_props: ConfigurationEditorProps) {
  return <p className="muted">The GitHub PR association is shared by the session. Set its repository and PR number or URL inside the tab.</p>;
}
export function PullRequestContent({ tab, session, visible }: PluginContentProps) {
  const [documentVisible, setDocumentVisible] = useState(!document.hidden);
  useEffect(() => {
    const changed = () => setDocumentVisible(!document.hidden);
    document.addEventListener('visibilitychange', changed);
    return () => document.removeEventListener('visibilitychange', changed);
  }, []);
  const associationKey = JSON.stringify(session.pullRequest);
  const [target, setTarget] = useState(session.pullRequest?.repository ?? '');
  const [number, setNumber] = useState(String(session.pullRequest?.number ?? ''));
  const [data, setData] = useState<PullRequest>();
  const [lastRefresh, setLastRefresh] = useState<string>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const generation = useRef(0);
  const refreshRef = useRef<() => void>(() => {});
  async function refresh() {
    if (pending.current || !session.pullRequest) return;
    const request = generation.current;
    pending.current = true; setBusy(true); setError('');
    try {
      const result = await invoke<PullRequest>('refresh_pull_request', { id: tab.id });
      if (request === generation.current) { setData(result); setLastRefresh(new Date().toLocaleString()); }
    } catch (error) { if (request === generation.current) setError(String(error)); }
    finally { if (request === generation.current) { pending.current = false; setBusy(false); } }
  }
  refreshRef.current = () => { void refresh(); };
  useEffect(() => {
    setTarget(session.pullRequest?.repository ?? ''); setNumber(String(session.pullRequest?.number ?? ''));
    setData(undefined); setLastRefresh(undefined); setError('');
    pending.current = false; setBusy(false); ++generation.current;
    if (session.pullRequest) refreshRef.current();
    // Rust cancels and joins native work on association changes and closure.
    return () => { ++generation.current; };
  }, [tab.id, associationKey]);
  useEffect(() => {
    if (!visible || !documentVisible) return;
    const timer = setInterval(() => refreshRef.current(), 60_000);
    return () => clearInterval(timer);
  }, [visible, documentVisible]);
  async function associate() {
    setError('');
    try { await invoke('associate_pull_request', { id: tab.id, target, number: number ? Number(number) : null }); }
    catch (error) { setError(String(error)); }
  }
  return <section role="tabpanel" id={`tab-panel-${tab.id}`} aria-labelledby={`tab-${tab.id}`} hidden={!visible} className="plugin-panel">
    <form onSubmit={event => { event.preventDefault(); void associate(); }} className="plugin-toolbar">
      <input aria-label="GitHub repository or PR URL" placeholder="owner/repository or GitHub PR URL" value={target} onChange={event => setTarget(event.target.value)} />
      <input aria-label="PR number" type="number" min="1" placeholder="PR number" value={number} onChange={event => setNumber(event.target.value)} />
      <button>Save association</button>
    </form>
    <p className="muted">This association belongs to the session. Clear the repository to remove it.</p>
    <button disabled={busy || !session.pullRequest} onClick={() => void refresh()}>{busy ? 'Refreshing…' : 'Refresh'}</button>
    {error && <p role="alert" className="error">{error}</p>}
    {lastRefresh && <p className="muted">Last successful refresh: {lastRefresh}</p>}
    {data && <>
      <h2>#{data.number} {data.title}</h2>
      <p>State: {data.state} · Review: {data.reviewDecision || 'No review decision'}</p>
      <button onClick={() => void invoke('open_terminal_link', { uri: data.url }).catch(error => setError(String(error)))}>Open pull request</button>
      <h3>Checks</h3>
      {data.statusCheckRollup?.length ? <ul>{data.statusCheckRollup.map((check, index) => <li key={index}>{check.name ?? check.context}: {check.conclusion || check.state || check.status || 'Unknown'}</li>)}</ul> : <p>No checks reported.</p>}
    </>}
  </section>;
}
