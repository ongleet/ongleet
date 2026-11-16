import type { ChangeData } from 'react-diff-view';
import type { Thread } from './comments';

const empty: readonly never[] = [];
export type FileDiscussions = { file: readonly Thread[]; line: readonly Thread[] };
const emptyFile: FileDiscussions = { file: empty, line: empty };

/** Preserve a bucket's identity when its ordered, immutable thread references match. */
function retain<T>(previous: readonly T[], next: readonly T[]) {
  return previous.length === next.length && next.every((item, i) => item === previous[i])
    ? previous
    : next;
}

/** Index each published snapshot once; unrelated file/line revisions remain stable. */
export class DiscussionIndex {
  private source?: readonly Thread[];
  private files = new Map<string, FileDiscussions>();
  orphans: readonly Thread[] = empty;

  read(threads: readonly Thread[]) {
    if (threads === this.source) return this;
    const files = new Map<string, { file: Thread[]; line: Thread[] }>();
    const orphans: Thread[] = [];
    for (const thread of threads) {
      if (thread.orphan) {
        orphans.push(thread);
        continue;
      }
      const path = thread.location.path;
      let bucket = files.get(path);
      if (!bucket) files.set(path, (bucket = { file: [], line: [] }));
      bucket[thread.location.side ? 'line' : 'file'].push(thread);
    }
    const next = new Map<string, FileDiscussions>();
    for (const [path, bucket] of files) {
      const previous = this.forFile(path);
      const file = retain(previous.file, bucket.file);
      const line = retain(previous.line, bucket.line);
      next.set(path, file === previous.file && line === previous.line ? previous : { file, line });
    }
    this.orphans = retain(this.orphans, orphans);
    this.files = next;
    this.source = threads;
    return this;
  }

  forFile(path: string): FileDiscussions {
    return this.files.get(path) ?? emptyFile;
  }
}

type LineLocation = { side: 'old' | 'new'; line: number };
type Entry<T> = { order: number; item: T };

/** Find widgets directly by side/line, preserving cross-side order on context rows. */
export class LineDiscussionIndex<T extends LineLocation> {
  private old = new Map<number, Entry<T>[]>();
  private new = new Map<number, Entry<T>[]>();

  constructor(items: readonly T[]) {
    items.forEach((item, order) => {
      const side = this[item.side];
      let bucket = side.get(item.line);
      if (!bucket) side.set(item.line, (bucket = []));
      bucket.push({ order, item });
    });
  }

  forChange(change: ChangeData): readonly T[] {
    if (change.type !== 'normal') {
      const side = change.type === 'delete' ? this.old : this.new;
      return side.get(change.lineNumber)?.map((entry) => entry.item) ?? empty;
    }
    const old = this.old.get(change.oldLineNumber) ?? empty;
    const next = this.new.get(change.newLineNumber) ?? empty;
    if (!old.length && !next.length) return empty;
    const result: T[] = [];
    let i = 0,
      j = 0;
    while (i < old.length || j < next.length) {
      if (j === next.length || (i < old.length && old[i].order < next[j].order))
        result.push(old[i++].item);
      else result.push(next[j++].item);
    }
    return result;
  }
}
