import { SdkError } from '../../sdk';
export type ReviewState = { expanded: boolean; viewed: boolean };
export type ReviewFile = { path: string; key: string; patch?: string; failed: boolean };
/** Memory-only tab review state. A changed comparison resets all marks. */
export class ReviewStore {
  revision = crypto.randomUUID();
  private identity = '';
  private files: ReviewFile[] = [];
  private states = new Map<string, ReviewState>();
  reconcile(identity: string, files: ReviewFile[]) {
    const reset = identity !== this.identity;
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
      next.set(
        file.key,
        !reset && old && state && (!state.viewed || (!file.failed && old.patch === file.patch))
          ? state
          : { expanded: true, viewed: false },
      );
    }
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
      this.states.set(
        file.key,
        field === 'viewed' ? { viewed: value, expanded: !value } : { ...previous, expanded: value },
      );
    }
    return { updated: files.length };
  }
}
