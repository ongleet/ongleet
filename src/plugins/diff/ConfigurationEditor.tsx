import { Input, Select } from '../../components/Controls';
import { Checkbox } from '../../components/Checkbox';
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
        <Select
          value={config.mode}
          onChange={(e) => onChange(changeMode(config, e.target.value as Settings['mode']))}
        >
          <option value="branch">Branch changes</option>
          <option value="refs">Between two refs</option>
        </Select>
      </label>
      <datalist id={referenceList}>
        {references.map((ref) => (
          <option key={ref} value={ref} />
        ))}
      </datalist>
      {config.mode === 'branch' ? (
        <>
          <label>
            Compare from
            <Select
              value={config.compareFrom}
              onChange={(e) => onChange({ ...config, compareFrom: e.target.value })}
            >
              <option value="divergence">Branch divergence (default)</option>
              <option value="HEAD">Latest commit</option>
              {!['divergence', 'HEAD'].includes(config.compareFrom) &&
                !commits.some((commit) => commit.hash === config.compareFrom) && (
                  <option value={config.compareFrom}>{config.compareFrom}</option>
                )}
              {commits.map((commit) => (
                <option key={commit.hash} value={commit.hash}>
                  {commit.hash.slice(0, 10)} · {commit.subject}
                </option>
              ))}
            </Select>
            {commitError && <span role="alert">Could not load commits: {commitError}</span>}
          </label>
          <label className="diff-uncommitted-field">
            <Checkbox
              checked={config.includeUncommitted}
              onChange={(e) => onChange({ ...config, includeUncommitted: e.target.checked })}
            />
            Include uncommitted changes
          </label>
          <p className="diff-comparison-help">
            Compare the starting snapshot with the latest commit. Local changes include staged,
            unstaged, and untracked files.
          </p>
        </>
      ) : (
        <>
          <label>
            From ref
            <Input
              required
              list={referenceList}
              value={config.baseRef}
              onChange={(e) => onChange({ ...config, baseRef: e.target.value })}
            />
          </label>
          <span aria-hidden="true">→</span>
          <label>
            To ref
            <Input
              required
              list={referenceList}
              value={config.targetRef}
              onChange={(e) => onChange({ ...config, targetRef: e.target.value })}
            />
          </label>
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
      <Input
        value={config.folder ?? ''}
        placeholder="Session directory"
        onChange={(e) => onChange(JSON.stringify({ ...config, folder: e.target.value || null }))}
      />
    </label>
  );
}
