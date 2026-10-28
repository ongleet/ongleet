import type { ConfigurationEditorProps } from '..';
import { changeMode, defaults, type Settings } from './settings';
export function ComparisonFields({
  config,
  onChange,
}: {
  config: Settings;
  onChange: (config: Settings) => void;
}) {
  return (
    <>
      <label>
        Repository folder
        <input
          value={config.folder ?? ''}
          placeholder="Session directory"
          onChange={(e) => onChange({ ...config, folder: e.target.value || null })}
        />
      </label>
      <label>
        Comparison
        <select
          value={config.mode}
          onChange={(e) => onChange(changeMode(config, e.target.value as Settings['mode']))}
        >
          <option value="workingDirectory">Working directory</option>
          <option value="refs">Two refs</option>
          <option value="mergeBase">Merge base</option>
        </select>
      </label>
      <label>
        Base ref
        <input
          required
          value={config.baseRef}
          onChange={(e) => onChange({ ...config, baseRef: e.target.value })}
        />
      </label>
      {config.mode !== 'workingDirectory' && (
        <label>
          Destination ref
          <input
            required
            value={config.targetRef}
            onChange={(e) => onChange({ ...config, targetRef: e.target.value })}
          />
        </label>
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
  return <ComparisonFields config={config} onChange={(c) => onChange(JSON.stringify(c))} />;
}
