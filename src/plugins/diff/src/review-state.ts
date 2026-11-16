import { SdkError } from '../../../sdk';

export type ReviewState = { expanded: boolean; viewed: boolean };

export type ReviewFile = { path: string; key: string; patch?: string; failed: boolean };

/** Identify why a patch needs explicit loading before rendering. */
export function largePatchReason(patch: string | undefined): 'bytes' | 'lines' | undefined {
  if (!patch) return undefined;
  if (new TextEncoder().encode(patch).length > 64 * 1024) return 'bytes';
  let lines = 1;
  for (let i = 0; i < patch.length; i++) {
    if (patch[i] === '\n' && i < patch.length - 1 && ++lines > 500) return 'lines';
  }
  return undefined;
}

/** Memory-only tab review state. A changed comparison resets all marks. */
export class ReviewStore {
  revision = crypto.randomUUID();
  private identity = '';
  private files: ReviewFile[] = [];
  private states = new Map<string, ReviewState>();
  private expandedByUser = new Set<string>();

  /** Retain valid review choices across refreshes and reset them for a different comparison. */
  reconcile(identity: string, files: ReviewFile[]) {
    const reset = identity !== this.identity;
    if (reset) this.expandedByUser.clear();
    if (
      reset ||
      files.length !== this.files.length ||
      files.some(
        (file, i) =>
          file.key !== this.files[i]?.key ||
          file.patch !== this.files[i]?.patch ||
          file.failed !== this.files[i]?.failed,
      )
    ) {
      this.revision = crypto.randomUUID();
    }
    const previous = new Map(this.files.map((file) => [file.key, file]));
    const next = new Map<string, ReviewState>();
    for (const file of files) {
      const old = previous.get(file.key);
      const state = this.states.get(file.key);
      const retain =
        !reset &&
        old &&
        state &&
        ((!file.failed && old.patch === file.patch) ||
          (!state.viewed && this.expandedByUser.has(file.key)));
      if (!retain) this.expandedByUser.delete(file.key);
      const retained = retain && (state.viewed || this.expandedByUser.has(file.key));
      // A changed reviewed patch must be visible again, including large patches.
      const reopen = !reset && state?.viewed && !retain;
      if (reopen) this.expandedByUser.add(file.key);
      const expanded = reopen || !largePatchReason(file.patch);
      next.set(file.key, retained ? state : { expanded, viewed: false });
    }
    this.expandedByUser = new Set([...this.expandedByUser].filter((key) => next.has(key)));
    this.states = next;
    this.identity = identity;
    this.files = files;
  }

  /** Return the current marks, defaulting unknown files to expanded and unviewed. */
  get(key: string): ReviewState {
    return this.states.get(key) ?? { expanded: true, viewed: false };
  }

  /** Validate the entire batch before changing marks; viewed updates require the current revision. */
  set(paths: string[], field: 'expanded' | 'viewed', value: boolean, revision?: string) {
    if (field === 'viewed' && revision !== this.revision)
      throw new SdkError('revision_conflict', 'Comparison changed; read state again');
    const files = paths.map((path) => {
      const file = this.files.find((file) => file.path === path);
      if (!file) throw new SdkError('invalid_arguments', `Unknown comparison path: ${path}`);
      if (field === 'viewed' && (file.failed || !file.patch))
        throw new SdkError('unavailable', `Patch unavailable: ${path}`);
      return file;
    });
    for (const file of files) {
      const previous = this.get(file.key);
      this.expandedByUser.add(file.key);
      this.states.set(
        file.key,
        field === 'viewed' ? { viewed: value, expanded: !value } : { ...previous, expanded: value },
      );
    }
    return { updated: files.length };
  }
}
