import { useId } from 'react';
import type { CommitOption } from './git';
import type { ConfigurationEditorProps } from '..';
import { changeMode, defaults, type Settings } from './settings';
export function ComparisonFields({
  config,
  onChange,
  references = [],
  commits = [],
  commitError = '',
}: {
  config: Settings;
  onChange: (config: Settings) => void;
  references?: string[];
  commits?: CommitOption[];
  commitError?: string;
}) {
  const referenceList = useId();
  return (
    <>
      <label className="diff-mode-field">
        Comparison
        <select
          value={config.mode}
          onChange={(e) => onChange(changeMode(config, e.target.value as Settings['mode']))}
        >
          <option value="auto">Automatic (committed only)</option>
          <option value="workingDirectory">Uncommitted changes</option>
          <option value="branch">Branch + uncommitted changes</option>
          <option value="sinceCommit">Since commit + uncommitted changes</option>
          <option value="refs">Between two references</option>
          <option value="mergeBase">Branch commits (merge base)</option>
        </select>
      </label>
      <p className="diff-comparison-help">
        {
          {
            auto: 'Committed changes only: branch changes, or the latest commit on the default branch.',
            workingDirectory:
              'Base reference → current files. Use HEAD for staged and unstaged changes only. Untracked files excluded.',
            branch:
              'Common ancestor of the base branch and HEAD → current files, including staged and unstaged changes. Untracked files excluded.',
            sinceCommit:
              'Selected commit → current files: later commits plus staged and unstaged changes. The selected commit itself and untracked files are excluded. Latest 200 commits from HEAD.',
            refs: 'Base reference → destination reference. Compares committed snapshots; excludes local changes.',
            mergeBase:
              'Common ancestor of both references → destination reference. Committed changes only.',
          }[config.mode]
        }
      </p>
      <datalist id={referenceList}>
        {references.map((ref) => (
          <option key={ref} value={ref} />
        ))}
      </datalist>
      {config.mode === 'sinceCommit' && (
        <label>
          Base commit
          <select
            value={config.baseRef}
            onChange={(e) => onChange({ ...config, baseRef: e.target.value })}
          >
            {!commits.some((commit) => commit.hash === config.baseRef) && (
              <option value={config.baseRef}>
                {config.baseRef === 'HEAD' ? 'HEAD (latest commit)' : config.baseRef}
              </option>
            )}
            {commits.map((commit) => (
              <option key={commit.hash} value={commit.hash}>
                {commit.hash.slice(0, 10)} · {commit.subject}
              </option>
            ))}
          </select>
          {commitError && <span role="alert">Could not load commits: {commitError}</span>}
        </label>
      )}
      {config.mode !== 'auto' && config.mode !== 'sinceCommit' && (
        <>
          <label>
            {config.mode === 'branch' ? 'Base branch' : 'Base reference'}
            <input
              required
              list={referenceList}
              value={config.baseRef}
              onChange={(e) => onChange({ ...config, baseRef: e.target.value })}
            />
          </label>
          {config.mode !== 'workingDirectory' && config.mode !== 'branch' && (
            <>
              <span aria-hidden="true">→</span>
              <label>
                Destination reference
                <input
                  required
                  list={referenceList}
                  value={config.targetRef}
                  onChange={(e) => onChange({ ...config, targetRef: e.target.value })}
                />
              </label>
            </>
          )}
        </>
      )}
    </>
  );
}
export function ConfigurationEditor({ value, onChange }: ConfigurationEditorProps) {
  let config = defaults;
  try {
    config = { ...defaults, ...JSON.parse(value) };
  } catch {
    /* Host displays malformed JSON. */
  }
  return (
    <label>
      Repository folder
      <input
        value={config.folder ?? ''}
        placeholder="Session directory"
        onChange={(e) => onChange(JSON.stringify({ ...config, folder: e.target.value || null }))}
      />
    </label>
  );
}
