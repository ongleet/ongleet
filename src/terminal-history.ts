export const TERMINAL_HISTORY_SEPARATOR = '--- Previous terminal history; new process below ---';

// Only recognize standalone path prompts. Arbitrary repeated program output must
// remain intact, even when it happens to be identical across process restarts.
function pathPrompt(text: string): string | undefined {
  const line = text.trim();
  return /^(?:[^\s:@]+@[^\s:]+:)?(?:~(?:\/[^\n$#%>❯]*)?|\/[^\n$#%>❯]*)[$#%>❯]$/.test(line)
    ? line
    : undefined;
}

export function compactTerminalHistory(text: string): string {
  const sections = text.split(new RegExp(`(?:^|\n)${TERMINAL_HISTORY_SEPARATOR}\n`));
  const kept: string[] = [];
  let previousPrompt: string | undefined;
  let onlyPrompt = true;
  for (const section of sections) {
    const trimmed = section.trim();
    if (sections.length > 1 && !trimmed) continue;
    const prompt = pathPrompt(trimmed);
    if (prompt && !trimmed.includes('\n')) {
      if (prompt === previousPrompt) continue;
    } else {
      onlyPrompt = false;
    }
    kept.push(section);
    previousPrompt = pathPrompt(trimmed.split('\n').at(-1) ?? '');
  }
  // A shell which has only displayed its initial prompt has no work to restore.
  if (onlyPrompt && kept.length === 1) return '';
  return kept.join(`\n${TERMINAL_HISTORY_SEPARATOR}\n`);
}

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
