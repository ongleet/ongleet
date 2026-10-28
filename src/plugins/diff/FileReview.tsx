import { useEffect, useState, type ReactNode } from 'react';

export function FileReview({
  path,
  status,
  patch,
  failed,
  children,
}: {
  path: string;
  status: string;
  patch: string | undefined;
  failed: boolean;
  children: ReactNode;
}) {
  const [reviewedPatch, setReviewedPatch] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(true);
  const viewed =
    reviewedPatch !== null && !failed && (patch === undefined || reviewedPatch === patch);

  useEffect(() => {
    if (reviewedPatch !== null && (failed || (patch !== undefined && patch !== reviewedPatch))) {
      setReviewedPatch(null);
      setExpanded(true);
    }
  }, [patch, failed, reviewedPatch]);

  return (
    <>
      <header>
        <button
          type="button"
          className="diff-fold"
          aria-label={`${expanded ? 'Collapse' : 'Expand'} ${path}`}
          aria-expanded={expanded}
          onClick={() => setExpanded((value) => !value)}
        >
          <span aria-hidden="true">{expanded ? '▾' : '▸'}</span>
        </button>
        <span className="diff-file-name">
          <span className="diff-status">{status}</span> {path}
        </span>
        <label className="diff-viewed">
          <input
            type="checkbox"
            checked={viewed}
            disabled={patch === undefined || failed || !patch}
            aria-label={`Mark ${path} as viewed`}
            onChange={(event) => {
              setReviewedPatch(event.target.checked ? patch! : null);
              setExpanded(!event.target.checked);
            }}
          />
          Viewed
        </label>
      </header>
      {expanded && <div className="diff-file-content">{children}</div>}
    </>
  );
}
