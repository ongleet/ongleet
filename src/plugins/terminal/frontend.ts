import type { PluginSdk, PtyOutput as Output, ProcessStatus } from '../../sdk';
import {
  FitAddon,
  Ghostty,
  Terminal,
  LinkDetector,
  OSC8LinkProvider,
  UrlRegexProvider,
} from 'ghostty-web';
import wasmUrl from 'ghostty-web/ghostty-vt.wasm?url';
import { consumeOutput, initializeTerminal, SCROLLBACK_LINES } from '../../byte-transport';
import { suppressHiddenRendering } from '../../render-visibility';
import { createEngineLoader } from '../../engine-loader';
import { attachTerminalMouse } from '../../terminal-mouse';
import { TerminalLinks } from '../../terminal-links';

const loadGhostty = createEngineLoader(() => Ghostty.load(wasmUrl));

export type { ProcessStatus } from '../../sdk';
export class TerminalFrontend {
  private static instances = new Set<TerminalFrontend>();

  static async saveAllHistory() {
    await Promise.all([...this.instances].map((terminal) => terminal.saveHistory()));
  }

  private term?: Terminal;
  private observer?: ResizeObserver;
  private subscriptions: { dispose(): void }[] = [];
  private id?: number;
  private timer?: ReturnType<typeof setTimeout>;
  private inputQueue: Promise<void> = Promise.resolve();
  private queuedInput = 0;
  private pasting = false;
  private visible = true;
  private fitVisible?: () => void;
  private replaying = false;
  private historyTimer?: ReturnType<typeof setTimeout>;
  private lastHistorySave = 0;
  private historySave?: Promise<void>;
  private disposed = false;
  private unregisterValidation?: () => void;

  constructor(
    private host: HTMLElement,
    private tabId: number,
    private changed: (status: ProcessStatus | null) => void,
    private error: (error: unknown) => void,
    private sdk: PluginSdk,
  ) {
    TerminalFrontend.instances.add(this);
    document.addEventListener('visibilitychange', this.visibilityChanged);
    if (import.meta.env.MODE === 'native-validation') {
      void import('../../native-validation').then(({ registerTerminal }) => {
        if (this.disposed) return;
        this.unregisterValidation = registerTerminal(this.tabId, {
          state: () => ({
            sessionId: this.id,
            replaying: this.replaying,
            cols: this.term?.cols,
            rows: this.term?.rows,
            scrollback: this.term?.wasmTerm?.getScrollbackLength(),
            alternate: this.term?.wasmTerm?.isAlternateScreen(),
          }),
          snapshot: () => this.snapshot(),
          selectAll: () => this.term?.selectAll(),
        });
      });
    }
  }

  private visibilityChanged = () => this.setVisible(this.visible);

  setVisible(visible: boolean) {
    const becameVisible = visible && !this.visible;
    this.visible = visible;
    if (!visible || document.hidden) {
      this.term?.blur();
      return;
    }
    if (!this.replaying) this.fitVisible?.();
    const term = this.term;
    if (term?.wasmTerm) term.renderer?.render(term.wasmTerm, true, term.viewportY, term);
    if (becameVisible) term?.focus();
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
    const term = new Terminal({
      ghostty,
      cols: 80,
      rows: 24,
      fontSize: 14,
      scrollback: SCROLLBACK_LINES,
      theme: {
        background: '#12161d',
        foreground: '#dce3ec',
        cursor: '#a3e6cc',
        selectionBackground: '#355369',
      },
    });
    this.term = term;
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(this.host);
    const terminalLinks = new TerminalLinks(
      term,
      (bytes) => consumeOutput(term, bytes),
      (title) => this.sdk.tab.setTitle(title),
    );
    this.subscriptions.push(
      attachTerminalMouse(
        this.host,
        term,
        (bytes) => this.sendBytes(bytes),
        () => !this.replaying && this.visible && !this.disposed,
      ),
    );
    const redraw = () => {
      if (term.wasmTerm && this.visible)
        term.renderer?.render(term.wasmTerm, true, term.viewportY, term);
    };
    this.subscriptions.push(
      term.onSelectionChange(() => {
        redraw();
        void this.copy().catch(this.error);
      }),
    );
    const paste = (event: ClipboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      void this.paste().catch(this.error);
    };
    this.host.addEventListener('paste', paste, true);
    const copy = (event: ClipboardEvent) => {
      if (!term.getSelection()) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      void this.copy().catch(this.error);
    };
    this.host.addEventListener('copy', copy, true);
    const links = new LinkDetector(term);
    links.registerProvider(new OSC8LinkProvider(term));
    links.registerProvider(new UrlRegexProvider(term));
    const click = (event: MouseEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.button !== 0 || this.replaying) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      const canvas = this.host.querySelector('canvas');
      if (!canvas || !term.renderer || !term.wasmTerm) return;
      const rect = canvas.getBoundingClientRect();
      const col = Math.floor((event.clientX - rect.left) / term.renderer.charWidth);
      const row =
        term.wasmTerm.getScrollbackLength() -
        Math.floor(term.viewportY) +
        Math.floor((event.clientY - rect.top) / term.renderer.charHeight);
      links.invalidateCache();
      void links
        .getLinkAt(col, row)
        .then((link) => {
          if (link && this.term === term && !this.disposed)
            return this.sdk.desktop.openUrl(link.text);
        })
        .catch(this.error);
    };
    this.host.addEventListener('click', click, true);
    this.subscriptions.push({
      dispose: () => {
        this.host.removeEventListener('paste', paste, true);
        this.host.removeEventListener('copy', copy, true);
        this.host.removeEventListener('click', click, true);
        links.dispose();
      },
    });
    if (term.renderer)
      suppressHiddenRendering(term.renderer, () => this.visible && !document.hidden);
    const fitVisible = () => {
      if (
        !this.replaying &&
        this.visible &&
        !document.hidden &&
        this.host.clientWidth > 0 &&
        this.host.clientHeight > 0
      )
        fit.fit();
    };
    this.fitVisible = fitVisible;
    fitVisible();
    initializeTerminal(term);
    // Clear the canvas immediately too; do not display recycled cells until
    // the library's next animation frame catches up with the reset VT state.
    if (term.wasmTerm) term.renderer?.render(term.wasmTerm, true, term.viewportY, term);
    this.observer = new ResizeObserver(fitVisible);
    this.observer.observe(this.host);
    this.sdk.tab.setTitle(null);
    this.subscriptions.push(term.onData((text) => this.send(text)));
    this.subscriptions.push(
      term.onResize(({ cols, rows }) => {
        if (this.id !== undefined && !this.replaying)
          void this.sdk.pty.resize(this.id!, cols, rows).catch(this.error);
      }),
    );
    // ghostty-web's custom handler returns true when it handled the event.
    term.attachCustomKeyEventHandler((event) => {
      // Codex reads image data from the native clipboard on Ctrl+V. Upstream
      // otherwise lets WebKit swallow this key as a text-only browser paste.
      if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key.toUpperCase() === 'V') {
        this.send('\x16');
        return true;
      }
      if (event.ctrlKey && event.shiftKey && ['C', 'V'].includes(event.key.toUpperCase())) {
        if (event.key.toUpperCase() === 'C') void this.copy().catch(this.error);
        else void this.paste().catch(this.error);
        return true;
      }
      return false;
    });
    if (import.meta.env.MODE === 'native-validation') {
      const { waitForConnection } = await import('../../native-validation');
      await waitForConnection(this.tabId);
      if (this.disposed || this.term !== term) return;
    }
    const history = await this.sdk.pty.history().catch((error) => {
      this.error(error);
      return '';
    });
    if (this.disposed || this.term !== term) return;
    let started: Promise<number | undefined>;
    const receive = (event: Output) => {
      void (async () => {
        const id = await started;
        if (id === undefined || this.id !== id || this.term !== term || this.disposed) return;
        if (event.type === 'data') {
          // Preserve byte boundaries all the way into Ghostty's stateful VT
          // parser. Never decode each PTY chunk as an independent UTF-8 string.
          terminalLinks.write(event.bytes);
          if (!this.replaying) this.scheduleHistory();
          // 0.4.0 parses synchronously; its write callback waits for rAF, which
          // can stop in a hidden window. ACK parsing, not painting.
          await this.sdk.pty.acknowledge(id, event.seq);
        } else if (event.type === 'size') {
          term.resize(event.cols, event.rows);
          await this.sdk.pty.acknowledge(id, event.seq);
        } else if (event.type === 'ready') {
          this.replaying = false;
          fitVisible();
          await this.sdk.pty.acknowledge(id, event.seq);
        } else if (event.type === 'eof') {
          await this.saveHistory();
        } else if (event.type === 'error') this.error(event.message);
      })().catch(this.error);
    };
    started = this.sdk.pty
      .connect(
        {
          cols: term.cols,
          rows: term.rows,
          startIfStopped,
        },
        receive,
      )
      .then((connection) => {
        if (this.disposed || this.term !== term) return undefined;
        if (history) {
          // Render saved text as text: never replay control sequences from disk.
          const safe = history.replace(/[\x00-\x09\x0b-\x1f\x7f-\x9f]/g, '');
          term.write(
            new TextEncoder().encode(
              safe.replace(/\n/g, '\r\n') +
                '\r\n--- Previous terminal history; new process below ---\r\n',
            ),
          );
        }
        if (!connection) return undefined;
        this.replaying = connection.replay;
        if (connection.replay) term.resize(connection.cols, connection.rows);
        // Publish the ID before waiting output handlers resume.
        this.id = connection.id;
        return connection.id;
      });
    try {
      await started;
      if (this.disposed || this.id === undefined) return;
      const status = await this.sdk.pty.status(this.id);
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

  private scheduleHistory() {
    if (this.historyTimer !== undefined) return;
    this.historyTimer = setTimeout(
      () => {
        void this.saveHistory()
          .catch(this.error)
          .finally(() => {
            this.historyTimer = undefined;
          });
      },
      Math.max(0, 1000 - (Date.now() - this.lastHistorySave)),
    );
  }

  async saveHistory() {
    // Keep disk writes ordered and retain at most one snapshot in flight.
    while (this.historySave) await this.historySave.catch(() => {});
    const term = this.term;
    const id = this.id;
    if (!term || id === undefined || this.replaying || this.disposed) return;
    this.lastHistorySave = Date.now();
    const buffer = term.buffer.active;
    const lines: string[] = [];
    let budget = 60000; // At most 240 KiB UTF-8, below the native 256 KiB limit.
    for (
      let row = buffer.length - 1;
      row >= 0 && lines.length < SCROLLBACK_LINES && budget > 0;
      row--
    ) {
      const line = (buffer.getLine(row)?.translateToString(true) ?? '')
        .slice(-budget)
        .replace(/^[\uDC00-\uDFFF]/, '');
      lines.push(line);
      budget -= line.length + 1;
    }
    const text = lines.reverse().join('\n').trimEnd();
    this.historySave = this.sdk.pty.saveHistory(id, text);
    try {
      await this.historySave;
    } finally {
      this.historySave = undefined;
    }
  }

  private poll(id: number) {
    this.timer = setTimeout(() => {
      void this.sdk.pty
        .status(id)
        .then((status) => {
          if (this.id !== id) return;
          this.changed(status);
          if (status.state === 'running') this.poll(id);
        })
        .catch(this.error);
    }, 300);
  }

  private send(text: string) {
    if (this.id === undefined || this.replaying || this.disposed) return;
    // Count UTF-8 bytes, including bracketed-paste framing. Reject a whole
    // oversized paste instead of silently truncating commands.
    if (text.length > 65536) {
      this.error('Input queue limit (64 KiB) exceeded');
      return;
    }
    this.sendBytes(new TextEncoder().encode(text));
  }

  private sendBytes(bytes: Uint8Array) {
    if (this.id === undefined || this.replaying || this.disposed) return;
    if (this.queuedInput + bytes.length > 65536) {
      this.error('Input queue limit (64 KiB) exceeded');
      return;
    }
    const id = this.id;
    this.queuedInput += bytes.length;
    this.inputQueue = this.inputQueue
      .then(async () => {
        if (this.id !== id) return;
        for (let i = 0; i < bytes.length; i += 8192) {
          await this.sdk.pty.input(id, Array.from(bytes.subarray(i, i + 8192)));
        }
      })
      .catch(this.error)
      .finally(() => {
        this.queuedInput -= bytes.length;
      });
  }

  async copy() {
    const text = this.term?.getSelection();
    if (text) await this.sdk.desktop.writeText(text);
  }

  async paste() {
    const term = this.term;
    const id = this.id;
    if (!term || id === undefined || this.replaying || this.pasting) return;
    this.pasting = true;
    try {
      const result = await this.sdk.desktop.clipboard();
      if (this.term !== term || this.id !== id || this.replaying || this.disposed) return;
      if (result.kind === 'image') this.send('\x16');
      else term.paste(result.text);
      term.focus();
    } finally {
      this.pasting = false;
    }
  }

  snapshot(): string {
    if (!this.term || this.id === undefined)
      throw new Error('Snapshot unavailable: no connected terminal frontend');
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
    await this.sdk.pty.close(this.id);
    this.id = undefined;
    clearTimeout(this.timer);
    this.changed(null);
    this.disposeEmulator();
  }

  private disposeEmulator() {
    clearTimeout(this.historyTimer);
    this.historyTimer = undefined;
    this.observer?.disconnect();
    this.subscriptions.splice(0).forEach((s) => s.dispose());
    this.term?.dispose();
    this.term = undefined;
    this.fitVisible = undefined;
    this.host.replaceChildren();
  }

  dispose() {
    TerminalFrontend.instances.delete(this);
    this.disposed = true;
    document.removeEventListener('visibilitychange', this.visibilityChanged);
    this.unregisterValidation?.();
    this.id = undefined;
    clearTimeout(this.timer);
    this.disposeEmulator();
    // Detaching a frontend must not implicitly kill a backend process.
  }
}
