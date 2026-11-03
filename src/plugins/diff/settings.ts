export type Settings = {
  folder: string | null;
  mode: 'branch' | 'refs';
  baseRef: string;
  targetRef: string;
  compareFrom: string;
  includeUncommitted: boolean;
};
export const defaults: Settings = {
  folder: null,
  mode: 'branch',
  baseRef: 'HEAD~1',
  targetRef: 'HEAD',
  compareFrom: 'divergence',
  includeUncommitted: false,
};
export function settings(value: unknown): Settings {
  const saved = value as Partial<Settings> & { mode?: string };
  const config = { ...defaults, ...saved };
  // Keep existing tabs usable while replacing the old comparison controls.
  const oldMode = (value as { mode?: string })?.mode;
  if (oldMode && ['auto', 'workingDirectory', 'mergeBase', 'sinceCommit'].includes(oldMode)) {
    config.mode = 'branch';
    config.compareFrom = ['workingDirectory', 'sinceCommit'].includes(oldMode)
      ? config.baseRef
      : 'divergence';
    config.includeUncommitted = ['workingDirectory', 'sinceCommit'].includes(oldMode);
  } else if (oldMode === 'branch' && !Object.hasOwn(value as object, 'compareFrom')) {
    config.includeUncommitted = true;
  }
  if (!['branch', 'refs'].includes(config.mode)) throw new Error('Choose a comparison mode.');
  if (typeof config.includeUncommitted !== 'boolean')
    throw new Error('Choose whether to include uncommitted changes.');
  if (
    config.folder !== null &&
    (typeof config.folder !== 'string' ||
      !config.folder.startsWith('/') ||
      config.folder.includes('\0'))
  )
    throw new Error('Folder must be an absolute directory.');
  for (const ref of [config.baseRef, config.targetRef, config.compareFrom])
    if (typeof ref !== 'string' || !ref.trim() || ref.startsWith('-') || /[\0\r\n]/.test(ref))
      throw new Error('Enter a nonempty revision that does not start with a dash.');
  return config;
}
export function changeMode(config: Settings, mode: Settings['mode']): Settings {
  return { ...config, mode };
}
