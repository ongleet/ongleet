// Snapshot only after output, at most once per five seconds. Explicit lifecycle
// flushes bypass the timer. A generation distinguishes output arriving in flight.
export class TerminalHistory {
  private revision = 0;
  private savedRevision = 0;
  private savedText?: string;
  private timer?: ReturnType<typeof setTimeout>;
  private pending?: Promise<void>;
  private disposed = false;

  private snapshot: () => string | undefined;
  private persist: (text: string) => Promise<void>;
  private error: (error: unknown) => void;

  constructor(
    snapshot: () => string | undefined,
    persist: (text: string) => Promise<void>,
    error: (error: unknown) => void,
  ) {
    this.snapshot = snapshot;
    this.persist = persist;
    this.error = error;
  }

  changed() {
    if (this.disposed) return;
    this.revision++;
    this.schedule();
  }

  private schedule() {
    if (this.disposed || this.timer !== undefined || this.pending) return;
    this.timer = setTimeout(() => {
      this.timer = undefined;
      void this.flush().catch(this.error);
    }, 5000);
  }

  async flush() {
    clearTimeout(this.timer);
    this.timer = undefined;
    // Concurrent flushes serialize and recheck dirtiness after the earlier save.
    while (this.pending) await this.pending.catch(() => {});
    clearTimeout(this.timer);
    this.timer = undefined;
    if (this.disposed || this.savedRevision === this.revision) return;
    const revision = this.revision;
    const text = this.snapshot();
    if (text === undefined) return;
    if (text === this.savedText) {
      this.savedRevision = revision;
      return;
    }
    const pending = Promise.resolve().then(() => this.persist(text));
    this.pending = pending;
    try {
      await pending;
      this.savedText = text;
      this.savedRevision = revision;
    } finally {
      this.pending = undefined;
      if (this.savedRevision !== this.revision) this.schedule();
    }
  }

  dispose() {
    this.disposed = true;
    clearTimeout(this.timer);
    this.timer = undefined;
  }
}
