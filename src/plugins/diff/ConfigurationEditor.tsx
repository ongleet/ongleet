import { useId } from 'react';
import type { ConfigurationEditorProps } from '..';
import { changeMode, defaults, type Settings } from './settings';
export function ComparisonFields({
  config,
  onChange,
  references = [],
}: {
  config: Settings;
  onChange: (config: Settings) => void;
  references?: string[];
}) {
  const referenceList = useId();
  return (
    <>
      <label>
        Comparison
        <select
          value={config.mode}
          onChange={(e) => onChange(changeMode(config, e.target.value as Settings['mode']))}
        >
          <option value="auto">Automatic</option>
          <option value="workingDirectory">Working directory</option>
          <option value="refs">Two references</option>
          <option value="mergeBase">Merge base</option>
        </select>
      </label>
      <datalist id={referenceList}>
        {references.map((ref) => (
          <option key={ref} value={ref} />
        ))}
      </datalist>
      {config.mode !== 'auto' && (
        <>
          <label>
            Base reference
            <input
              required
              list={referenceList}
              value={config.baseRef}
              onChange={(e) => onChange({ ...config, baseRef: e.target.value })}
            />
          </label>
          {config.mode !== 'workingDirectory' && (
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
