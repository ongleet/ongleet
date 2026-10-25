export const DEFAULT_REFRESH_SECONDS = 120;
export function refreshSeconds(config: unknown): number {
  const seconds = (config as { refreshIntervalSeconds?: unknown } | null)?.refreshIntervalSeconds;
  if (seconds === undefined) return DEFAULT_REFRESH_SECONDS;
  if (typeof seconds !== 'number' || !Number.isInteger(seconds) || seconds < 30 || seconds > 86400)
    throw new Error('Refresh interval must be a whole number from 30 to 86400 seconds');
  return seconds;
}
