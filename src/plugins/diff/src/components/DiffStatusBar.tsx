import { t, getLocale } from '../../../../i18n';
import type { Comparison } from '../git';

/** Summarize the last successful comparison even while a replacement is loading or failed. */
export function DiffStatusBar({
  comparison,
  filter,
  loading,
  error,
}: {
  comparison: Comparison;
  filter: string;
  loading: boolean;
  error: string;
}) {
  return (
    <footer className="plugin-statusbar diff-statusbar" aria-label={t('Diff status')}>
      <div className="diff-summary">
        {
          {
            branch: 'Branch changes',
            refs: 'Between two refs',
          }[comparison.config.mode]
        }{' '}
        · {comparison.baseLabel} ({comparison.base.slice(0, 12)}) →{' '}
        {comparison.target
          ? `${comparison.targetLabel} (${comparison.target.slice(0, 12)})`
          : t('Working directory')}{' '}
        {t('· Last successful refresh ')}
        {new Date(comparison.time).toLocaleTimeString(getLocale())}
        {error && t(' · Stale')}
      </div>
      <span>
        {comparison.files.length}
        {t(' changed files ·')}{' '}
        {
          comparison.files.filter((file) =>
            `${file.oldPath}\n${file.path}`.toLowerCase().includes(filter.toLowerCase()),
          ).length
        }{' '}
        {t('matching filter')}
      </span>
      {(loading || error) && (
        <span>
          {loading ? t('Loading comparison…') : t('Refresh failed · Showing previous comparison')}
        </span>
      )}
    </footer>
  );
}
