import type { ConfigurationEditorProps } from '..';
export function FilesConfigurationEditor({ value, onChange }: ConfigurationEditorProps) {
  let config: { folder?: string; editorFile?: string } = {};
  let folder = '';
  try {
    config = JSON.parse(value);
    folder = config.folder ?? '';
  } catch {
    /* The YAML view reports malformed configuration. */
  }
  return (
    <label>
      Initial folder
      <input
        placeholder="Session directory"
        value={folder}
        onChange={(event) =>
          onChange(JSON.stringify({ ...config, folder: event.target.value || undefined }))
        }
      />
    </label>
  );
}
