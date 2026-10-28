export type Settings = {
  folder: string | null;
  mode: 'workingDirectory' | 'refs' | 'mergeBase';
  baseRef: string;
  targetRef: string;
};
export const defaults: Settings = {
  folder: null,
  mode: 'workingDirectory',
  baseRef: 'HEAD',
  targetRef: 'HEAD',
};
export function settings(value: unknown): Settings {
  const config = { ...defaults, ...(value as Partial<Settings>) };
  if (!['workingDirectory', 'refs', 'mergeBase'].includes(config.mode))
    throw new Error('Choose a comparison mode.');
  if (
    config.folder !== null &&
    (typeof config.folder !== 'string' ||
      !config.folder.startsWith('/') ||
      config.folder.includes('\0'))
  )
    throw new Error('Folder must be an absolute directory.');
  for (const ref of [config.baseRef, config.targetRef])
    if (typeof ref !== 'string' || !ref.trim() || ref.startsWith('-') || /[\0\r\n]/.test(ref))
      throw new Error('Enter a nonempty revision that does not start with a dash.');
  return config;
}
export function changeMode(config: Settings, mode: Settings['mode']): Settings {
  return {
    ...config,
    mode,
    ...(config.mode === 'workingDirectory' &&
    config.baseRef === 'HEAD' &&
    config.targetRef === 'HEAD' &&
    mode !== 'workingDirectory'
      ? { baseRef: mode === 'refs' ? 'HEAD~1' : 'main' }
      : {}),
  };
}
