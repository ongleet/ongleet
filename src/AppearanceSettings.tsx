import { defaultApplicationAppearance, type ApplicationAppearance } from './application-settings';
import { t, type Locale } from './i18n';
import { Button, Select } from './components/Controls';
import { themePresets } from './appearance';
import { findTheme, type ThemeId, type ThemePreset } from './theme-presets';

function Preview({ theme, terminal = false }: { theme: ThemePreset; terminal?: boolean }) {
  const colors = terminal
    ? theme.terminal
    : { ...theme.terminal, background: theme.ui.editor, foreground: theme.ui.text };
  return (
    <div
      className="appearance-preview"
      style={{ background: colors.background, color: colors.foreground }}
    >
      <div
        className="appearance-swatches"
        aria-label={t('{value0} color swatches', { value0: theme.name })}
      >
        {[
          colors.background,
          colors.foreground,
          colors.red,
          colors.green,
          colors.yellow,
          colors.blue,
          colors.magenta,
          colors.cyan,
        ].map((color, i) => (
          <span key={i} style={{ background: color }} />
        ))}
      </div>
      {terminal ? (
        <pre>
          <span style={{ color: colors.green }}>$</span> echo "Hello, Ongleet"{'\n'}Hello, Ongleet
          <span style={{ background: colors.cursor, color: colors.cursorAccent }}> </span>
        </pre>
      ) : (
        <pre>
          <span style={{ color: theme.syntax.keyword }}>const</span> greeting ={' '}
          <span style={{ color: theme.syntax.string }}>"Hello, Ongleet"</span>;{'\n'}
          <span style={{ color: theme.syntax.function }}>print</span>(greeting);
        </pre>
      )}
    </div>
  );
}
export function AppearanceSettings({
  value,
  onChange,
  disabled,
}: {
  value: ApplicationAppearance;
  onChange: (value: ApplicationAppearance) => void;
  disabled: boolean;
}) {
  const update = (patch: Partial<ApplicationAppearance>) => onChange({ ...value, ...patch });
  return (
    <fieldset disabled={disabled} className="appearance-settings">
      <h2 id="appearance-title">{t('Appearance')}</h2>
      <p>{t('Appearance and language apply when you save the configuration.')}</p>
      <label>
        {t('Language')}
        <Select
          value={value.language}
          onChange={(event) => update({ language: event.target.value as Locale })}
        >
          <option value="en" lang="en">
            English
          </option>
          <option value="fr" lang="fr">
            Français
          </option>
        </Select>
      </label>
      <label>
        {t('Application theme')}
        <Select
          value={value.application}
          onChange={(event) => update({ application: event.target.value as ThemeId })}
        >
          {themePresets.map((theme) => (
            <option key={theme.id} value={theme.id}>
              {theme.name}
            </option>
          ))}
        </Select>
      </label>
      <Preview theme={findTheme(value.application)!} />
      <label>
        {t('Terminal theme')}
        <Select
          value={value.terminal ?? ''}
          onChange={(event) => update({ terminal: (event.target.value as ThemeId) || null })}
        >
          <option value="">{t('Follow application')}</option>
          {themePresets.map((theme) => (
            <option key={theme.id} value={theme.id}>
              {theme.name}
            </option>
          ))}
        </Select>
      </label>
      <Preview theme={findTheme(value.terminal ?? value.application)!} terminal />
      <p>
        {t(
          'Existing terminals update cursor, selection and canvas background. Ghostty 0.4.0 keeps existing cell colors; the full palette applies when a terminal opens.',
        )}
      </p>
      <div className="dialog-actions">
        <Button type="button" onClick={() => onChange({ ...defaultApplicationAppearance })}>
          {t('Restore defaults')}
        </Button>
      </div>
    </fieldset>
  );
}
