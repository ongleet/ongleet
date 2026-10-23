import type { ConfigurationEditorProps } from '..';
export function FilesConfigurationEditor({ value, onChange }: ConfigurationEditorProps) {
  let folder = '';
  try { folder = JSON.parse(value).folder ?? ''; } catch { /* The YAML view reports malformed configuration. */ }
  return <label>Initial folder<input placeholder="Session directory" value={folder} onChange={event => onChange(JSON.stringify(event.target.value ? { folder: event.target.value } : {}))} /></label>;
}
