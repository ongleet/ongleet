import { SdkError, type Namespace } from '../../sdk';

export const MAX_NOTE_BYTES = 24 * 1024;
type Note = { text: string };
type Storage = {
  read: () => Promise<Namespace<Note> | null>;
  replace: (revision: number, value: Note) => Promise<Namespace<Note>>;
};

/** Serializes autosaves and keeps edits made while a write is pending. */
export class NoteModel {
  text = '';
  savedText = '';
  revision = 0;
  loading = true;
  error = '';
  observedAt: string | null = null;
  private pending: Promise<void> | null = null;
  constructor(
    private storage: Storage,
    private changed: () => void,
  ) {}
  get dirty() {
    return this.text !== this.savedText;
  }
  get saving() {
    return this.pending !== null;
  }

  async load(discard = false) {
    if ((!discard && this.dirty) || this.pending || (this.loading && this.observedAt))
      throw new SdkError('busy', 'Save pending edits first.');
    this.loading = true;
    this.changed();
    try {
      const note = await this.storage.read();
      if (note && (note.version !== 1 || typeof note.value.text !== 'string'))
        throw new SdkError('unavailable', 'Unsupported saved Note format.');
      this.text = this.savedText = note?.value.text ?? '';
      this.revision = note?.revision ?? 0;
      this.observedAt = new Date().toISOString();
      this.error = '';
    } catch (error) {
      this.error = String(error);
      throw error;
    } finally {
      this.loading = false;
      this.changed();
    }
  }

  edit(text: string) {
    if (this.loading) throw new SdkError('busy', 'Note is loading.');
    if (new TextEncoder().encode(text).length > MAX_NOTE_BYTES)
      throw new SdkError('limit_exceeded', 'Notes are limited to 24 KiB of UTF-8 text.');
    if (new TextEncoder().encode(JSON.stringify({ text })).length > 48 * 1024)
      throw new SdkError('limit_exceeded', 'Note exceeds the encoded storage limit.');
    this.text = text;
    this.changed();
  }

  save(): Promise<void> {
    if (this.pending) return this.pending;
    // Defer to a microtask so pending is set before the first write starts.
    const work = Promise.resolve().then(async () => {
      try {
        while (this.dirty) {
          const text = this.text;
          const result = await this.storage.replace(this.revision, { text });
          this.savedText = text;
          this.revision = result.revision;
          this.observedAt = new Date().toISOString();
          this.changed();
        }
        this.error = '';
      } catch (error) {
        this.error = String(error);
        throw error;
      } finally {
        this.pending = null;
        this.changed();
      }
    });
    this.pending = work;
    this.changed();
    return work;
  }

  async replace(text: string, revision: number, signal: AbortSignal) {
    signal.throwIfAborted();
    if (this.loading || this.pending || this.dirty || this.error)
      throw new SdkError('busy', 'Note is loading, has unsaved edits, or needs reloading.');
    if (revision !== this.revision)
      throw new SdkError('revision_conflict', 'Read Note state again before replacing its text.');
    this.edit(text);
    await this.save();
    return { revision: this.revision };
  }
}
