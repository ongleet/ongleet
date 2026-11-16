import { SdkError } from '../../../sdk';
export type ReviewState = { expanded: boolean; viewed: boolean };
export type ReviewFile = { path: string; key: string; patch?: string; failed: boolean };
function startsExpanded(patch: string | undefined) {
  if (!patch) return true;
  if (new TextEncoder().encode(patch).length > 64 * 1024) return false;
  let lines = 1;
  for (let i = 0; i < patch.length; i++) {
    if (patch[i] === '\n' && i < patch.length - 1 && ++lines > 500) return false;
  }
  return true;
}
/** Memory-only tab review state. A changed comparison resets all marks. */
export class ReviewStore {
  revision = crypto.randomUUID();
  private identity = '';
  private files: ReviewFile[] = [];
  private states = new Map<string, ReviewState>();
  private expandedByUser = new Set<string>();
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
      next.set(file.key, retain ? state : { expanded: startsExpanded(file.patch), viewed: false });
    }
    this.expandedByUser = new Set([...this.expandedByUser].filter((key) => next.has(key)));
    this.states = next;
    this.identity = identity;
    this.files = files;
  }
  get(key: string): ReviewState {
    return this.states.get(key) ?? { expanded: true, viewed: false };
  }
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
