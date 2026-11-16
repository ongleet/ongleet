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
}: {
  files: FileChange[];
  selection: DiffSelection;
  visible: boolean;
  onSelect: (file: FileChange) => void;
  labels: Record<string, string>;
}) {
  const selected = useSyncExternalStore(selection.subscribe, selection.getSnapshot);
  useDiffRenderDiagnostic('FileTree', { files, selected, visible });
  const root = useMemo(() => buildFileTree(files), [files]);

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
        </li>
      ))}
    </ul>
  );
  return render(root);
}
