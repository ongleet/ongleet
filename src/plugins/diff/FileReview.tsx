import { t } from '../../i18n';
import { Button } from '../../components/Controls';
import { Checkbox } from '../../components/Checkbox';
import type { ReactNode } from 'react';

export function FileReview({
  comment,
  path,
  status,
  statusCode,
  patch,
  failed,
  children,
  expanded,
  viewed,
  setExpanded,
  setViewed,
}: {
  comment: () => void;
  path: string;
  status: string;
  statusCode: string;
  patch: string | undefined;
  failed: boolean;
  children: ReactNode;
  expanded: boolean;
  viewed: boolean;
  setExpanded: (value: boolean) => void;
  setViewed: (value: boolean) => void;
}) {
  return (
    <>
      <header>
        <Button
          type="button"
          className="diff-fold"
          aria-label={t(expanded ? 'Collapse {path}' : 'Expand {path}', { path })}
          aria-expanded={expanded}
          onClick={() => setExpanded(!expanded)}
        >
          <span aria-hidden="true">{expanded ? '▾' : '▸'}</span>
        </Button>
        <span className="diff-file-name">
          <span className="diff-status" data-status={statusCode}>
            {status}
          </span>{' '}
          {path}
        </span>
        <Button onClick={comment}>{t('Comment')}</Button>
        <label className="diff-viewed">
          <Checkbox
            checked={viewed}
            disabled={patch === undefined || failed || !patch}
            aria-label={t('Mark {value0} as viewed', { value0: path })}
            onChange={(event) => {
              setViewed(event.target.checked);
            }}
          />
          {t('Viewed')}
        </label>
      </header>
      {expanded && <div className="diff-file-content">{children}</div>}
    </>
  );
}
