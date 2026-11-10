import { t } from '../../i18n';
import { Checkbox } from '../../components/Checkbox';
import { Input, Select } from '../../components/Controls';
import type { ConfigurationEditorProps } from '..';
export type LaunchDraft = {
  mode: 'executable' | 'shell';
  executable: string;
  args: string;
  env: string;
  cwd: string;
  shell: string;
};
export function LaunchConfigurationEditor({
  action,
  patch,
}: {
  action: LaunchDraft;
  patch: (changes: Partial<LaunchDraft>) => void;
}) {
  return (
    <>
      <label>
        {t('Launch mode')}
        <Select
          value={action.mode}
          onChange={(event) => patch({ mode: event.target.value as LaunchDraft['mode'] })}
        >
          <option value="executable">{t('Executable + arguments')}</option>
          <option value="shell">{t('Shell command')}</option>
        </Select>
      </label>
      {action.mode === 'executable' ? (
        <>
          <label>
            {t('Executable')}
            <Input
              aria-label={t('Executable')}
              value={action.executable}
              onChange={(event) => patch({ executable: event.target.value })}
            />
          </label>
          <label>
            {t('Arguments (JSON array)')}
            <Input
              aria-label={t('Arguments')}
              value={action.args}
              onChange={(event) => patch({ args: event.target.value })}
            />
          </label>
        </>
      ) : (
        <label>
          {t('Shell command')}
          <Input
            aria-label={t('Shell command')}
            value={action.shell}
            onChange={(event) => patch({ shell: event.target.value })}
          />
        </label>
      )}
      <label>
        {t('Directory override')}
        <Input
          value={action.cwd}
          placeholder={t('Session directory')}
          onChange={(event) => patch({ cwd: event.target.value })}
        />
      </label>
      <label>
        {t('Environment overrides (JSON)')}
        <Input
          aria-label={t('Environment overrides')}
          value={action.env}
          onChange={(event) => patch({ env: event.target.value })}
        />
      </label>
    </>
  );
}

export function launchDraft(config: unknown): string {
  const value = config as {
    executable?: string;
    args?: string[];
    env?: Record<string, string>;
    cwdOverride?: string;
    shellCommand?: string;
  };
  return JSON.stringify({
    mode: value.shellCommand == null ? 'executable' : 'shell',
    executable: value.executable ?? '',
    args: JSON.stringify(value.args ?? []),
    env: JSON.stringify(value.env ?? {}),
    cwd: value.cwdOverride ?? '',
    shell: value.shellCommand ?? '',
  });
}
export function launchConfig(text: string): unknown {
  const action = JSON.parse(text) as LaunchDraft;
  const args: unknown = action.mode === 'executable' ? JSON.parse(action.args) : [];
  const env: unknown = JSON.parse(action.env);
  if (!Array.isArray(args) || !args.every((value) => typeof value === 'string'))
    throw new Error(t('Arguments must be a JSON array of strings.'));
  if (
    !env ||
    typeof env !== 'object' ||
    Array.isArray(env) ||
    !Object.values(env).every((value) => typeof value === 'string')
  )
    throw new Error(t('Environment must be a JSON object of strings.'));
  return {
    executable: action.mode === 'executable' ? action.executable : null,
    args,
    env,
    cwdOverride: action.cwd.trim() || null,
    shellCommand: action.mode === 'shell' ? action.shell : null,
  };
}
export function terminalDraft(config: unknown): string {
  return JSON.stringify({
    ...JSON.parse(launchDraft(config)),
    loadOnDemand: (config as { loadOnDemand?: boolean }).loadOnDemand === true,
  });
}
export function terminalConfig(text: string): unknown {
  return {
    ...(launchConfig(text) as object),
    loadOnDemand: JSON.parse(text).loadOnDemand === true,
  };
}

export function TerminalConfigurationEditor({ value, onChange }: ConfigurationEditorProps) {
  const action = JSON.parse(value) as LaunchDraft & { loadOnDemand?: boolean };
  return (
    <>
      <LaunchConfigurationEditor
        action={action}
        patch={(changes) => onChange(JSON.stringify({ ...action, ...changes }))}
      />
      <label>
        <Checkbox
          checked={action.loadOnDemand === true}
          onChange={(event) =>
            onChange(JSON.stringify({ ...action, loadOnDemand: event.target.checked }))
          }
        />
        {t('Load on demand')}
      </label>
      <p className="muted">
        {t('Start when first shown or requested through the API. Keep running afterward.')}
      </p>
    </>
  );
}

export function CommandConfigurationEditor({ value, onChange }: ConfigurationEditorProps) {
  const action = JSON.parse(value) as LaunchDraft;
  return (
    <LaunchConfigurationEditor
      action={action}
      patch={(changes) => onChange(JSON.stringify({ ...action, ...changes }))}
    />
  );
}
