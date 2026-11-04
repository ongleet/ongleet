import type { ReactNode } from 'react';

export function FileReview({
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
        <button
          type="button"
          className="diff-fold"
          aria-label={`${expanded ? 'Collapse' : 'Expand'} ${path}`}
          aria-expanded={expanded}
          onClick={() => setExpanded(!expanded)}
        >
          <span aria-hidden="true">{expanded ? '▾' : '▸'}</span>
        </button>
        <span className="diff-file-name">
          <span className="diff-status" data-status={statusCode}>
            {status}
          </span>{' '}
          {path}
        </span>
        <label className="diff-viewed">
          <input
            type="checkbox"
            checked={viewed}
            disabled={patch === undefined || failed || !patch}
            aria-label={`Mark ${path} as viewed`}
            onChange={(event) => {
              setViewed(event.target.checked);
            }}
          />
          Viewed
        </label>
      </header>
      {expanded && <div className="diff-file-content">{children}</div>}
    </>
  );
}
