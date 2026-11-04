import type { PluginSdk } from '../../sdk';
import { SdkError } from '../../sdk';
export type Location = {
  root: string;
  path: string;
  side?: 'old' | 'new';
  line?: number;
  context: string[];
  targetOffset?: number;
};
export type Comment = { id: string; text: string; origin: 'User' | 'Agent'; at: string };
export type Thread = {
  id: string;
  original: Location;
  location: Location;
  orphan: boolean;
  comments: Comment[];
};
export type CommentFile = { path: string; oldPath: string; patch?: string; failed?: boolean };
const prefix = 'ongleet.diff-comments.v1.';
const bytes = (text: string) => new TextEncoder().encode(text).length;
export function patchLines(patch: string, side: 'old' | 'new') {
  const result: { line: number; text: string }[] = [];
  let old = 0,
    next = 0,
    active = false;
  for (const text of patch.split('\n')) {
    const header = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(text);
    if (header) {
      old = +header[1];
      next = +header[2];
      active = true;
      continue;
    }
    if (!active || !/^[ +\-]/.test(text)) continue;
    if (text[0] === ' ' || (side === 'old' ? text[0] === '-' : text[0] === '+'))
      result.push({ line: side === 'old' ? old : next, text: text.slice(1) });
    if (text[0] !== '+') old++;
    if (text[0] !== '-') next++;
  }
  return result;
}
function validLocation(value: Location) {
  return (
    value &&
    typeof value.root === 'string' &&
    typeof value.path === 'string' &&
    bytes(value.root) <= 4096 &&
    bytes(value.path) <= 4096 &&
    (value.side === undefined
      ? value.line === undefined
      : (value.side === 'old' || value.side === 'new') &&
        Number.isInteger(value.line) &&
        value.line! > 0) &&
    Array.isArray(value.context) &&
    value.context.every((line) => typeof line === 'string') &&
    bytes(JSON.stringify(value.context)) <= 4096
  );
}
function validThreads(threads: Thread[]) {
  const ids = new Set<string>();
  return threads.every((thread) => {
    if (
      !thread ||
      typeof thread.id !== 'string' ||
      ids.has(thread.id) ||
      !validLocation(thread.original) ||
      !validLocation(thread.location) ||
      typeof thread.orphan !== 'boolean' ||
      !Array.isArray(thread.comments) ||
      !thread.comments.length
    )
      return false;
    ids.add(thread.id);
    return thread.comments.every((comment) => {
      if (
        !comment ||
        typeof comment.id !== 'string' ||
        ids.has(comment.id) ||
        typeof comment.text !== 'string' ||
        !comment.text.trim() ||
        bytes(comment.text) > 4096 ||
        !['User', 'Agent'].includes(comment.origin) ||
        typeof comment.at !== 'string' ||
        !Number.isFinite(Date.parse(comment.at))
      )
        return false;
      ids.add(comment.id);
      return true;
    });
  });
}
export class CommentStore {
  threads: Thread[] = [];
  error = '';
  private key: string;
  private storage?: Storage;
  constructor(sessionId: number, tabId: number, storage?: Storage) {
    this.key = `${prefix}${sessionId}.${tabId}`;
    try {
      this.storage = storage ?? localStorage;
      const raw = this.storage.getItem(this.key);
      if (raw) {
        if (bytes(raw) > 1024 * 1024) throw new Error('Stored discussion exceeds limit');
        const record = JSON.parse(raw);
        if (record.version !== 1 || !Array.isArray(record.threads) || !validThreads(record.threads))
          throw new Error('Invalid discussion record');
        this.threads = record.threads;
      }
    } catch (error) {
      this.error = `Could not load comments: ${String(error)}`;
    }
  }
  private commit(threads: Thread[]) {
    const raw = JSON.stringify({ version: 1, threads });
    if (bytes(raw) > 1024 * 1024)
      throw new SdkError('invalid_arguments', 'Discussion storage limit reached (1 MiB)');
    try {
      if (!this.storage) throw new Error('Local storage unavailable');
      this.storage.setItem(this.key, raw);
      this.error = '';
    } catch (error) {
      this.error = `Could not save comments: ${String(error)}`;
      throw new SdkError('unavailable', this.error);
    }
    this.threads = threads;
  }
  private comment(text: string, origin: 'User' | 'Agent'): Comment {
    if (!text.trim() || bytes(text) > 4096)
      throw new SdkError('invalid_arguments', 'Comment must contain text and fit in 4 KiB');
    return { id: crypto.randomUUID(), text, origin, at: new Date().toISOString() };
  }
  get(id: string) {
    const thread = this.threads.find((t) => t.id === id);
    if (!thread) throw new SdkError('invalid_arguments', 'Unknown thread ID');
    return thread;
  }
  add(location: Location, text: string, origin: 'User' | 'Agent') {
    if (!validLocation(location))
      throw new SdkError('invalid_arguments', 'Location exceeds comment bounds');
    const thread: Thread = {
      id: crypto.randomUUID(),
      original: location,
      location,
      orphan: false,
      comments: [this.comment(text, origin)],
    };
    this.commit([...this.threads, thread]);
    return { threadId: thread.id, commentId: thread.comments[0].id };
  }
  reply(id: string, text: string, origin: 'User' | 'Agent') {
    const thread = this.get(id),
      comment = this.comment(text, origin);
    this.commit(
      this.threads.map((t) => (t === thread ? { ...t, comments: [...t.comments, comment] } : t)),
    );
    return { threadId: id, commentId: comment.id };
  }
  delete(id: string) {
    this.get(id);
    this.commit(this.threads.filter((t) => t.id !== id));
    return { deleted: true };
  }
  anchor(root: string, file: CommentFile, side?: 'old' | 'new', line?: number): Location {
    if (file.patch === undefined || file.failed)
      throw new SdkError('unavailable', 'Patch unavailable');
    if ((side === undefined) !== (line === undefined))
      throw new SdkError('invalid_arguments', 'Side and line must be supplied together');
    if (!side)
      return {
        root,
        path: file.path,
        context: [
          ...file.patch
            .split('\n')
            .slice(0, 12)
            .map((line) => line.slice(0, 160)),
        ],
      };
    const lines = patchLines(file.patch, side),
      index = lines.findIndex((l) => l.line === line);
    if (index < 0) throw new SdkError('invalid_arguments', 'Unknown diff line');
    const start = index > 0 && lines[index - 1].line === line! - 1 ? index - 1 : index;
    const end = lines[index + 1]?.line === line! + 1 ? index + 2 : index + 1;
    const context = lines.slice(start, end).map((l) => l.text);
    if (bytes(JSON.stringify(context)) > 4000)
      throw new SdkError('invalid_arguments', 'Line context exceeds 4 KiB');
    return { root, path: file.path, side, line, context, targetOffset: index - start };
  }
  reconcile(root: string, files: CommentFile[]) {
    const threads = this.threads.map((thread) => {
      const anchor = thread.original;
      const candidates =
        root === anchor.root
          ? files.filter(
              (f) =>
                f.path === anchor.path ||
                f.oldPath === anchor.path ||
                f.path === thread.location.path,
            )
          : [];
      const file = candidates.length === 1 ? candidates[0] : undefined;
      if (file && (file.patch === undefined || file.failed)) return thread;
      if (!file) return { ...thread, orphan: true };
      if (!anchor.side)
        return { ...thread, orphan: false, location: { ...anchor, path: file.path } };
      const lines = patchLines(file.patch!, anchor.side),
        context = anchor.context;
      const matches = lines.flatMap((_, i) =>
        context.every(
          (text, j) =>
            lines[i + j]?.text === text &&
            (j === 0 || lines[i + j].line === lines[i + j - 1].line + 1),
        )
          ? [i]
          : [],
      );
      if (matches.length !== 1 || !lines[matches[0] + (anchor.targetOffset ?? 0)])
        return { ...thread, orphan: true };
      return {
        ...thread,
        orphan: false,
        location: {
          ...anchor,
          path: file.path,
          line: lines[matches[0] + (anchor.targetOffset ?? 0)].line,
        },
      };
    });
    if (JSON.stringify(threads) !== JSON.stringify(this.threads)) this.commit(threads);
  }
  page(offset = 0, limit = 20) {
    // Page individual comments so even a long thread cannot exceed the response bound.
    const all = this.threads.flatMap((t) =>
      t.comments.map((comment) => ({
        threadId: t.id,
        original: t.original,
        location: t.location,
        orphan: t.orphan,
        comment,
      })),
    );
    const comments: typeof all = [];
    let size = 0;
    for (const item of all.slice(offset, offset + limit)) {
      const length = bytes(JSON.stringify(item));
      if (size + length > 32000) break;
      comments.push(item);
      size += length;
    }
    return {
      comments,
      totalComments: all.length,
      totalThreads: this.threads.length,
      nextCommentOffset: offset + comments.length < all.length ? offset + comments.length : null,
    };
  }
}
export function commentMessage(tabId: number, thread: Thread, commentId: string) {
  const selected = thread.comments.find((c) => c.id === commentId);
  if (!selected) throw new SdkError('invalid_arguments', 'Unknown comment ID');
  const location = thread.orphan ? thread.original : thread.location;
  const header = `Diff tab ${tabId}, thread ${thread.id}, selected comment ${selected.id}\nLocation: ${location.root}/${location.path} ${location.side ?? 'file'} ${location.line ?? ''}${thread.orphan ? ' (orphan)' : ''}\nReply using ongleet tab invoke ${tabId} reply-comment --args '{"threadId":"${thread.id}","text":"your reply"}'. Labels describe origin, not verified identity.\nSelected (${selected.origin}):\n${selected.text}\n`;
  let result = header;
  for (const comment of thread.comments.filter((c) => c.id !== commentId)) {
    const entry = `\n${comment.origin} ${comment.id}: ${comment.text}\n`;
    if (bytes(result + entry) < 7400) result += entry;
  }
  let excerpt = `\nCaptured code:\n${thread.original.context.join('\n')}`;
  while (bytes(result + excerpt) > 8191 && excerpt.length) excerpt = excerpt.slice(0, -1);
  if (bytes(result) > 8191)
    throw new SdkError('invalid_arguments', 'Location and comment exceed send limit');
  return result + excerpt;
}

export async function commentTargets(tabs: PluginSdk['tabs'], sessionId: number, tabId: number) {
  const list = await tabs.list({ sessionId });
  const candidates = await Promise.all(
    list.tabs
      .filter((t) => t.sessionId === sessionId && t.isOpen && t.id !== tabId)
      .map(async (t) => ({ ...t, capabilities: await tabs.capabilities(t.id).catch(() => null) })),
  );
  return candidates.filter(
    (t) => t.capabilities?.available && t.capabilities.availableActions.includes('send-text'),
  );
}
export async function deliverComment(
  tabs: PluginSdk['tabs'],
  sessionId: number,
  tabId: number,
  thread: Thread,
  commentId: string,
  targetTabId: number,
) {
  if (!(await commentTargets(tabs, sessionId, tabId)).some((t) => t.id === targetTabId))
    throw new SdkError('unavailable', 'Recipient unavailable');
  const result = (await tabs.invoke(targetTabId, 'send-text', {
    text: commentMessage(tabId, thread, commentId),
    enter: true,
  })) as { acknowledged?: boolean };
  if (result.acknowledged !== true) throw new SdkError('unavailable', 'Write not acknowledged');
  return { acknowledged: true };
}
