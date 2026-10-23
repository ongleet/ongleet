import type { ConfigurationEditorProps } from "..";
export type LaunchDraft = { mode: 'executable' | 'shell'; executable: string; args: string; env: string; cwd: string; shell: string };
export function LaunchConfigurationEditor({ action, patch }: { action: LaunchDraft; patch: (changes: Partial<LaunchDraft>) => void }) {
  return <>
        <label>Launch mode<select value={action.mode} onChange={event => patch({ mode: event.target.value as LaunchDraft['mode'] })}><option value="executable">Executable + arguments</option><option value="shell">Shell command</option></select></label>
        {action.mode === 'executable' ? <>
          <label>Executable<input aria-label="Executable" value={action.executable} onChange={event => patch({ executable: event.target.value })} /></label>
          <label>Arguments (JSON array)<input aria-label="Arguments" value={action.args} onChange={event => patch({ args: event.target.value })} /></label>
        </> : <label>Shell command<input aria-label="Shell command" value={action.shell} onChange={event => patch({ shell: event.target.value })} /></label>}
        <label>Directory override<input value={action.cwd} placeholder="Session directory" onChange={event => patch({ cwd: event.target.value })} /></label>
        <label>Environment overrides (JSON)<input aria-label="Environment overrides" value={action.env} onChange={event => patch({ env: event.target.value })} /></label>
  </>;
}

export function launchDraft(config: unknown): string {
  const value = config as { executable?: string; args?: string[]; env?: Record<string, string>; cwdOverride?: string; shellCommand?: string };
  return JSON.stringify({ mode: value.shellCommand == null ? 'executable' : 'shell', executable: value.executable ?? '', args: JSON.stringify(value.args ?? []), env: JSON.stringify(value.env ?? {}), cwd: value.cwdOverride ?? '', shell: value.shellCommand ?? '' });
}
export function launchConfig(text: string): unknown {
  const action = JSON.parse(text) as LaunchDraft;
  const args: unknown = action.mode === 'executable' ? JSON.parse(action.args) : [];
  const env: unknown = JSON.parse(action.env);
  if (!Array.isArray(args) || !args.every(value => typeof value === 'string')) throw new Error('Arguments must be a JSON array of strings.');
  if (!env || typeof env !== 'object' || Array.isArray(env) || !Object.values(env).every(value => typeof value === 'string')) throw new Error('Environment must be a JSON object of strings.');
  return { executable: action.mode === 'executable' ? action.executable : null, args, env, cwdOverride: action.cwd.trim() || null, shellCommand: action.mode === 'shell' ? action.shell : null };
}
export function TerminalConfigurationEditor({ value, onChange }: ConfigurationEditorProps) {
  const action = JSON.parse(value) as LaunchDraft;
  return <LaunchConfigurationEditor action={action} patch={changes => onChange(JSON.stringify({ ...action, ...changes }))} />;
}
