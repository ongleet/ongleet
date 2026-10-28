import type { FileChange } from './git';

export type FileTreeNode = { directories: Map<string, FileTreeNode>; files: FileChange[] };

export function buildFileTree(files: FileChange[]): FileTreeNode {
  const root: FileTreeNode = { directories: new Map(), files: [] };
  for (const file of files) {
    let node = root;
    for (const part of file.path.split('/').slice(0, -1)) {
      if (!node.directories.has(part))
        node.directories.set(part, { directories: new Map(), files: [] });
      node = node.directories.get(part)!;
    }
    node.files.push(file);
  }
  return root;
}

export function filesInTreeOrder(files: FileChange[]): FileChange[] {
  const result: FileChange[] = [];
  const visit = (node: FileTreeNode) => {
    for (const [, child] of [...node.directories].sort(([a], [b]) => a.localeCompare(b)))
      visit(child);
    result.push(...node.files);
  };
  visit(buildFileTree(files));
  return result;
}
