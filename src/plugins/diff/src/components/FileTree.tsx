import { t } from '../../../../i18n';
import type { Thread } from '../comments';
import type { DiscussionIndex } from '../discussion-index';
import { useMemo, useSyncExternalStore } from 'react';
import { Button, Disclosure, DisclosureTrigger } from '../../../../components/Controls';
import { FileIcon } from '../../../../components/FileIcon';
import { FolderIcon } from '../../../../components/FolderIcon';
import { buildFileTree, type FileTreeNode } from '../file-tree';
import type { FileChange } from '../git';
import type { DiffSelection } from '../selection-state';
import { useDiffRenderDiagnostic } from '../render-diagnostics';

/** Render directory groups with selection updates isolated from comparison state. */
export function FileTree({
  files,
  selection,
  visible,
  onSelect,
  labels,
  discussions,
  onComment,
}: {
  files: FileChange[];
  selection: DiffSelection;
  visible: boolean;
  onSelect: (file: FileChange) => void;
  labels: Record<string, string>;
  discussions: DiscussionIndex;
  onComment: (thread: Thread, commentId: string, file?: FileChange) => void;
}) {
  const selected = useSyncExternalStore(selection.subscribe, selection.getSnapshot);
  useDiffRenderDiagnostic('FileTree', { files, selected, visible });
  const root = useMemo(() => buildFileTree(files), [files]);

  const renderComments = (threads: readonly Thread[], file?: FileChange) => (
    <ul className="diff-tree-comments">
      {threads.flatMap((thread) => {
        const location = thread.orphan ? thread.original : thread.location;
        const label = location.side
          ? t('{side} line {line}', {
              side: location.side === 'old' ? t('Old') : t('New'),
              line: location.line!,
            })
          : t('File discussion');
        return thread.comments.map((comment) => (
          <li key={comment.id}>
            <Button
              title={`${location.path} · ${label} · ${comment.origin}: ${comment.text}`}
              onClick={() => onComment(thread, comment.id, file)}
            >
              <span className="diff-comment-bubble" aria-hidden="true" />
              <span className="file-tree-label">{comment.text.replace(/\s+/g, ' ')}</span>
              {location.line && (
                <small>
                  {location.side === 'old' ? '−' : '+'}
                  {location.line}
                </small>
              )}
            </Button>
          </li>
        ));
      })}
    </ul>
  );

  const render = (node: FileTreeNode) => (
    <ul>
      {[...node.directories]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([name, child]) => (
          <li key={name}>
            <Disclosure open>
              <DisclosureTrigger title={name}>
                <FolderIcon />
                <span className="diff-tree-label file-tree-label">{name}</span>
              </DisclosureTrigger>
              {render(child)}
            </Disclosure>
          </li>
        ))}
      {node.files.map((file) => (
        <li key={file.key}>
          <Button
            title={file.oldPath !== file.path ? `${file.oldPath} → ${file.path}` : file.path}
            aria-pressed={selected === file.key}
            aria-current={selected === file.key ? 'true' : undefined}
            onClick={() => onSelect(file)}
          >
            <FileIcon status={file.status[0]} statusLabel={labels[file.status[0]]} />
            <span className="diff-tree-label file-tree-label">{file.path.split('/').at(-1)}</span>
          </Button>
          {renderComments(
            [...discussions.forFile(file.path).file, ...discussions.forFile(file.path).line],
            file,
          )}
        </li>
      ))}
    </ul>
  );
  return (
    <>
      {render(root)}
      {discussions.orphans.length > 0 && (
        <Disclosure open>
          <DisclosureTrigger>{t('Orphan comments')}</DisclosureTrigger>
          {renderComments(discussions.orphans)}
        </Disclosure>
      )}
    </>
  );
}
