import { Channel, invoke } from '@tauri-apps/api/core';
import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager';
import { FitAddon, Ghostty, Terminal } from 'ghostty-web';
import wasmUrl from 'ghostty-web/ghostty-vt.wasm?url';
import { consumeOutput, SCROLLBACK_LINES } from './byte-transport';
import { suppressHiddenRendering } from './render-visibility';
import { createEngineLoader } from './engine-loader';

const loadGhostty = createEngineLoader(() => Ghostty.load(wasmUrl));

export type ProcessStatus = { state: 'running' } | { state: 'exited'; code: number };
type Output = { type: 'data'; seq: number; bytes: number[] } | { type: 'size'; seq: number; cols: number; rows: number } | { type: 'ready'; seq: number } | { type: 'eof' } | { type: 'error'; message: string };

export class TerminalFrontend {
  private term?: Terminal;
  private observer?: ResizeObserver;
  private subscriptions: { dispose(): void }[] = [];
  private id?: number;
  private timer?: ReturnType<typeof setTimeout>;
  private inputQueue: Promise<void> = Promise.resolve();
  private queuedInput = 0;
  private visible = true;
  private fitVisible?: () => void;
  private replaying = false;
  private disposed = false;

  constructor(private host: HTMLElement, private tabId: number, private changed: (status: ProcessStatus | null) => void, private error: (error: unknown) => void) {}

  setVisible(visible: boolean) {
    this.visible = visible;
    if (!visible) { this.term?.blur(); return; }
    if (!this.replaying) this.fitVisible?.();
    const term = this.term;
    if (term?.wasmTerm) term.renderer?.render(term.wasmTerm, true, term.viewportY, term);
  }

  async start(startIfStopped = true) {
    if (this.id !== undefined) throw new Error('Close the existing session first.');
    let ghostty: Ghostty;
    try {
      ghostty = await loadGhostty();
    } catch (cause) {
      const hint = import.meta.env.DEV
        ? ' Check that the development server is running: mise exec -- npm run tauri -- dev. Then close this tab and open a new one.'
        : ' Close this tab and open a new one. If the problem persists, restart the application.';
      throw new Error(`Unable to load the terminal engine.${hint}`, { cause });
    }
    if (this.disposed) return;
    this.disposeEmulator();
    const term = new Terminal({ ghostty, cols: 80, rows: 24, fontSize: 14, scrollback: SCROLLBACK_LINES,
      theme: { background: '#12161d', foreground: '#dce3ec', cursor: '#a3e6cc', selectionBackground: '#355369' } });
    this.term = term;
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(this.host);
    if (term.renderer) suppressHiddenRendering(term.renderer, () => this.visible);
    const fitVisible = () => {
      if (!this.replaying && this.visible && this.host.clientWidth > 0 && this.host.clientHeight > 0) fit.fit();
    };
    this.fitVisible = fitVisible;
    fitVisible();
    this.observer = new ResizeObserver(fitVisible);
    this.observer.observe(this.host);
    this.subscriptions.push(term.onData(text => this.send(text)));
    this.subscriptions.push(term.onResize(({ cols, rows }) => {
      if (this.id !== undefined && !this.replaying) void invoke('resize', { id: this.id, cols, rows }).catch(this.error);
    }));
    // ghostty-web's custom handler returns true when it handled the event.
    term.attachCustomKeyEventHandler(event => {
      if (event.ctrlKey && event.shiftKey && ['C', 'V'].includes(event.key.toUpperCase())) {
        if (event.key.toUpperCase() === 'C') void this.copy().catch(this.error);
        else void this.paste().catch(this.error);
        return true;
      }
      return false;
    });
    const output = new Channel<Output>();
    let started: Promise<number | undefined>;
    output.onmessage = event => {
      void (async () => {
        const id = await started;
        if (id === undefined || this.id !== id || this.term !== term || this.disposed) return;
        if (event.type === 'data') {
          // Preserve byte boundaries all the way into Ghostty's stateful VT
          // parser. Never decode each PTY chunk as an independent UTF-8 string.
          consumeOutput(term, event.bytes);
          // 0.4.0 parses synchronously; its write callback waits for rAF, which
          // can stop in a hidden window. ACK parsing, not painting.
          await invoke('acknowledge', { id, seq: event.seq });
        } else if (event.type === 'size') {
          term.resize(event.cols, event.rows);
          await invoke('acknowledge', { id, seq: event.seq });
        } else if (event.type === 'ready') {
          this.replaying = false;
          fitVisible();
          await invoke('acknowledge', { id, seq: event.seq });
        } else if (event.type === 'error') this.error(event.message);
      })().catch(this.error);
    };
    type Attachment = { id: number; cols: number; rows: number; replay: boolean };
    started = invoke<Attachment | null>('connect', { tabId: this.tabId, cols: term.cols, rows: term.rows, output, startIfStopped }).then(connection => {
      if (!connection || this.disposed || this.term !== term) return undefined;
      this.replaying = connection.replay;
      if (connection.replay) term.resize(connection.cols, connection.rows);
      // Publish the ID before waiting output handlers resume.
      this.id = connection.id;
      return connection.id;
    });
    try {
      await started;
      if (this.disposed || this.id === undefined) return;
      const status = await invoke<ProcessStatus>('status', { id: this.id });
      if (this.disposed || this.term !== term) return;
      this.changed(status);
      fitVisible();
      this.poll(this.id);
      if (this.visible) term.focus();
    } catch (error) {
      this.disposeEmulator();
      throw error;
    }
  }

  private poll(id: number) {
    this.timer = setTimeout(() => {
      void invoke<ProcessStatus>('status', { id }).then(status => {
        if (this.id !== id) return;
        this.changed(status);
        if (status.state === 'running') this.poll(id);
      }).catch(this.error);
    }, 300);
  }

  private send(text: string) {
    if (this.id === undefined || this.replaying || this.disposed) return;
    // Count UTF-8 bytes, including bracketed-paste framing. Reject a whole
    // oversized paste instead of silently truncating commands.
    if (text.length > 65536) { this.error('Input queue limit (64 KiB) exceeded'); return; }
    const bytes = new TextEncoder().encode(text);
    if (this.queuedInput + bytes.length > 65536) { this.error('Input queue limit (64 KiB) exceeded'); return; }
    const id = this.id;
    this.queuedInput += bytes.length;
    this.inputQueue = this.inputQueue.then(async () => {
      if (this.id !== id) return;
      for (let i = 0; i < bytes.length; i += 8192) {
        await invoke('input', { id, bytes: Array.from(bytes.subarray(i, i + 8192)) });
      }
    }).catch(this.error).finally(() => { this.queuedInput -= bytes.length; });
  }

  async copy() {
    const text = this.term?.getSelection();
    if (text) await writeText(text);
  }

  async paste() {
    const term = this.term;
    const id = this.id;
    if (!term || id === undefined) return;
    const text = await readText();
    if (this.term !== term || this.id !== id) return;
    term.paste(text);
    this.term?.focus();
  }

  snapshot(): string {
    if (!this.term || this.id === undefined) throw new Error('Snapshot unavailable: no connected terminal frontend');
    // The existing Ghostty buffer is authoritative. No second VT parser.
    const buffer = this.term.buffer.active;
    const lines: string[] = [];
    for (let row = Math.max(0, buffer.length - this.term.rows); row < buffer.length; row++) {
      lines.push(buffer.getLine(row)?.translateToString(true) ?? '');
    }
    return lines.join('\n').slice(0, 65536);
  }

  async close() {
    if (this.id === undefined) return;
    await invoke('close_session', { id: this.id, confirmed: true });
    this.id = undefined;
    clearTimeout(this.timer);
    this.changed(null);
    this.disposeEmulator();
  }

  private disposeEmulator() {
    this.observer?.disconnect();
    this.subscriptions.splice(0).forEach(s => s.dispose());
    this.term?.dispose();
    this.term = undefined;
    this.fitVisible = undefined;
    this.host.replaceChildren();
  }

  dispose() {
    this.disposed = true;
    this.id = undefined;
    clearTimeout(this.timer);
    this.disposeEmulator();
    // Detaching a frontend must not implicitly kill a backend process.
  }
}
