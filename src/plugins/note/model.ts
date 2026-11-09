import { SdkError, type Namespace } from '../../sdk';

export const MAX_NOTE_BYTES = 24 * 1024;
export type Note = { id: string; name: string; text: string };
export type Notebook = { notes: Note[]; selectedNoteId: string };
type Storage = {
  read: () => Promise<Namespace<unknown> | null>;
  replace: (revision: number, value: Notebook) => Promise<Namespace<Notebook>>;
};
const bytes = (value: string) => new TextEncoder().encode(value).length;

/** Serializes autosaves of the entire notebook, including selection and names. */
export class NoteModel {
  book: Notebook = { notes: [], selectedNoteId: '' };
  private saved = JSON.stringify(this.book);
  revision = 0;
  loading = true;
  error = '';
  observedAt: string | null = null;
  private pending: Promise<void> | null = null;
  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  constructor(
    private storage: Storage,
    private changed: () => void,
  ) {}
  get notes() {
    return this.book.notes;
  }
  get selectedNoteId() {
    return this.book.selectedNoteId;
  }
  get text() {
    return this.notes.find((note) => note.id === this.selectedNoteId)?.text ?? '';
  }
  get dirty() {
    return JSON.stringify(this.book) !== this.saved;
  }
  get saving() {
    return this.pending !== null;
  }

  getNote(id = this.selectedNoteId): Note {
    const note = this.notes.find((note) => note.id === id);
    if (!note) throw new SdkError('invalid_arguments', 'Unknown note ID.');
    return note;
  }

  async load(discard = false) {
    if ((!discard && this.dirty) || this.pending)
      throw new SdkError('busy', 'Save pending edits first.');
    this.cancelScheduledSave();
    this.loading = true;
    this.changed();
    let migrate = false;
    try {
      const record = await this.storage.read();
      let book: Notebook;
      if (!record || record.version === 1) {
        const legacy = record?.value as { text?: unknown } | undefined;
        if (record && typeof legacy?.text !== 'string')
          throw new SdkError('unavailable', 'Unsupported saved Note format.');
        const first = {
          id: crypto.randomUUID(),
          name: 'Note 1',
          text: (legacy?.text as string) ?? '',
        };
        book = { notes: [first], selectedNoteId: first.id };
        migrate = true;
      } else if (record.version === 2) {
        book = record.value as Notebook;
        this.validate(book);
      } else throw new SdkError('unavailable', 'Unsupported saved Note format.');
      this.book = book;
      this.saved = migrate ? '' : JSON.stringify(book);
      this.revision = record?.revision ?? 0;
      this.observedAt = new Date().toISOString();
      this.error = '';
    } catch (error) {
      this.error = String(error);
      throw error;
    } finally {
      this.loading = false;
      this.changed();
    }
    if (migrate) await this.save();
  }

  private validate(book: Notebook) {
    if (
      !book ||
      !Array.isArray(book.notes) ||
      !book.notes.length ||
      book.notes.length > 100 ||
      new Set(book.notes.map((note) => note.id)).size !== book.notes.length ||
      !book.notes.some((note) => note.id === book.selectedNoteId)
    )
      throw new SdkError('invalid_arguments', 'Invalid notebook or more than 100 notes.');
    for (const note of book.notes) {
      if (
        typeof note.id !== 'string' ||
        !note.id ||
        bytes(note.id) > 128 ||
        typeof note.name !== 'string' ||
        !note.name.trim() ||
        bytes(note.name) > 128 ||
        /[\u0000-\u001f\u007f]/.test(note.name)
      )
        throw new SdkError(
          'invalid_arguments',
          'Note names must be 1–128 UTF-8 bytes without control characters.',
        );
      if (typeof note.text !== 'string' || bytes(note.text) > MAX_NOTE_BYTES)
        throw new SdkError('limit_exceeded', 'Notes are limited to 24 KiB of UTF-8 text.');
    }
    if (bytes(JSON.stringify(book)) > 48 * 1024)
      throw new SdkError('limit_exceeded', 'Notebook exceeds the 48 KiB encoded storage limit.');
  }

  private update(book: Notebook) {
    if (this.loading) throw new SdkError('busy', 'Note is loading.');
    this.validate(book);
    this.book = book;
    this.changed();
  }
  edit(text: string, id = this.selectedNoteId) {
    this.getNote(id);
    this.update({
      ...this.book,
      notes: this.notes.map((note) => (note.id === id ? { ...note, text } : note)),
    });
  }
  rename(name: string, id = this.selectedNoteId) {
    this.getNote(id);
    this.update({
      ...this.book,
      notes: this.notes.map((note) => (note.id === id ? { ...note, name } : note)),
    });
  }
  select(id: string) {
    this.getNote(id);
    this.update({ ...this.book, selectedNoteId: id });
  }
  add(name = `Note ${this.notes.length + 1}`) {
    const id = crypto.randomUUID();
    this.update({ notes: [...this.notes, { id, name, text: '' }], selectedNoteId: id });
    return id;
  }

  scheduleSave() {
    this.cancelScheduledSave();
    this.saveTimer = setTimeout(() => {
      this.saveTimer = null;
      void this.save().catch(() => {});
    }, 500);
  }

  private cancelScheduledSave() {
    if (this.saveTimer !== null) clearTimeout(this.saveTimer);
    this.saveTimer = null;
  }

  save(): Promise<void> {
    this.cancelScheduledSave();
    if (this.pending) return this.pending;
    const work = Promise.resolve().then(async () => {
      try {
        while (this.dirty && this.saveTimer === null) {
          const snapshot = this.book;
          const result = await this.storage.replace(this.revision, snapshot);
          this.saved = JSON.stringify(snapshot);
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

  async prepareSend(noteId: string, capturedText: string): Promise<number> {
    const check = () => {
      if (this.loading || this.getNote(noteId).text !== capturedText)
        throw new SdkError(
          'revision_conflict',
          'The note changed. Close this dialog and capture it again.',
        );
    };
    check();
    await this.save();
    check();
    if (this.dirty)
      throw new SdkError('busy', 'The note changed. Close this dialog and capture it again.');
    return this.revision;
  }

  async act(revision: number, signal: AbortSignal, update: () => string) {
    signal.throwIfAborted();
    if (this.loading || this.pending || this.dirty || this.error)
      throw new SdkError('busy', 'Note is loading, has unsaved edits, or needs reloading.');
    if (revision !== this.revision)
      throw new SdkError('revision_conflict', 'Read Note state again before modifying notes.');
    const noteId = update();
    await this.save();
    return { revision: this.revision, noteId };
  }
  replace(text: string, revision: number, signal: AbortSignal, id = this.selectedNoteId) {
    return this.act(revision, signal, () => {
      this.edit(text, id);
      return id;
    });
  }
}
