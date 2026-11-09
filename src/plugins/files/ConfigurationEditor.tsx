import { t } from '../../i18n';
import { Input } from '../../components/Controls';
import { Checkbox } from '../../components/Checkbox';
import { BrowserFields } from './SessionConfiguration';
import { browserDefaults, type BrowserPreferences } from './session-settings';
import type { ConfigurationEditorProps } from '..';
export function FilesConfigurationEditor({ value, onChange }: ConfigurationEditorProps) {
  let config: {
    folder?: string;
    editorFile?: string;
    browserPreferences?: BrowserPreferences | null;
  } = {};
  let folder = '';
  try {
    config = JSON.parse(value);
    folder = config.folder ?? '';
  } catch {
    /* The YAML view reports malformed configuration. */
  }
  return (
    <>
      <label>
        {t('Initial folder')}
        <Input
          placeholder={t('Session directory')}
          value={folder}
          onChange={(event) =>
            onChange(JSON.stringify({ ...config, folder: event.target.value || undefined }))
          }
        />
      </label>
      <label className="checkbox-field">
        <Checkbox
          checked={!!config.browserPreferences}
          onChange={(event) =>
            onChange(
              JSON.stringify({
                ...config,
                browserPreferences: event.target.checked ? { ...browserDefaults } : null,
              }),
            )
          }
        />
        {t('Override browser preferences for this tab')}
      </label>
      {config.browserPreferences && (
        <BrowserFields
          value={config.browserPreferences}
          change={(patch) =>
            onChange(
              JSON.stringify({
                ...config,
                browserPreferences: { ...config.browserPreferences, ...patch },
              }),
            )
          }
        />
      )}
    </>
  );
}
