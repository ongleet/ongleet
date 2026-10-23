import type { Terminal } from 'ghostty-web';

// ghostty-web 0.4.0 exposes hyperlink IDs in cells, but getHyperlinkUri is a
// stub. Retain only bounded OSC 8 metadata; Ghostty still parses all VT bytes
// and remains authoritative for cell positions, wrapping and scrollback.
export class TerminalLinks {
  private state: 'ground' | 'escape' | 'csi' | 'osc' | 'oscEscape' | 'string' | 'stringEscape' =
    'ground';
  private osc: number[] = [];
  private overflow = false;
  private pendingUri?: string;
  private uris = new Map<number, string>();
  private uriBytes = 0;
  private term: Terminal;
  private consume: (bytes: number[]) => void;

  constructor(term: Terminal, consume: (bytes: number[]) => void) {
    this.term = term;
    this.consume = consume;
    if (term.wasmTerm) term.wasmTerm.getHyperlinkUri = (id) => this.uris.get(id) ?? null;
  }

  write(bytes: number[]) {
    let start = 0;
    for (let i = 0; i < bytes.length; i++) {
      const byte = bytes[i];
      const engine = this.term.wasmTerm;
      const bind =
        this.pendingUri !== undefined && this.state === 'ground' && byte >= 32 && byte !== 127;
      if (bind && engine) {
        if (start < i) this.consume(bytes.slice(start, i));
        const before = engine.getCursor();
        const cell = engine.getLine(before.y)?.[before.x];
        this.consume([byte]);
        start = i + 1;
        const after = engine.getCursor();
        // At bottom-margin wrap, the printed cell is on the new bottom row.
        const row = after.x < before.x ? after.y : before.y;
        const col = after.x < before.x ? Math.max(0, after.x - 1) : before.x;
        const printed = engine.getLine(row)?.[col];
        if (
          printed?.hyperlink_id &&
          (before.x !== after.x ||
            before.y !== after.y ||
            printed.hyperlink_id !== cell?.hyperlink_id ||
            printed.codepoint !== cell?.codepoint)
        ) {
          this.remember(printed.hyperlink_id, this.pendingUri!);
          this.pendingUri = undefined;
        }
      }
      if (this.state === 'ground') {
        if (byte === 27) this.state = 'escape';
      } else if (this.state === 'escape') {
        if (byte === 93) {
          this.state = 'osc';
          this.osc = [];
          this.overflow = false;
        } else if (byte === 91) this.state = 'csi';
        else if ([80, 88, 94, 95].includes(byte)) this.state = 'string';
        else {
          if (byte === 99) {
            this.uris.clear();
            this.uriBytes = 0;
            this.pendingUri = undefined;
          }
          this.state = byte === 27 ? 'escape' : 'ground';
        }
      } else if (this.state === 'csi') {
        if (byte >= 64 && byte <= 126) this.state = 'ground';
        else if (byte === 27) this.state = 'escape';
        else if (byte === 24 || byte === 26) this.state = 'ground';
      } else if (this.state === 'osc' || this.state === 'oscEscape') {
        if (byte === 7 || (this.state === 'oscEscape' && byte === 92)) {
          if (start <= i) this.consume(bytes.slice(start, i + 1));
          start = i + 1;
          if (!this.overflow) this.oscComplete();
          this.state = 'ground';
          this.osc = [];
        } else if (byte === 24 || byte === 26) {
          this.state = 'ground';
          this.osc = [];
        } else if (byte === 27) this.state = 'oscEscape';
        else {
          if (this.osc.length < 8192) this.osc.push(byte);
          else this.overflow = true;
          this.state = 'osc';
        }
      } else if (this.state === 'string') {
        if (byte === 27) this.state = 'stringEscape';
        else if (byte === 24 || byte === 26) this.state = 'ground';
      } else this.state = byte === 92 ? 'ground' : byte === 27 ? 'stringEscape' : 'string';
    }
    if (start < bytes.length) this.consume(bytes.slice(start));
  }

  private oscComplete() {
    const text = new TextDecoder().decode(new Uint8Array(this.osc));
    if (!text.startsWith('8;')) return;
    const separator = text.indexOf(';', 2);
    if (separator >= 0) this.pendingUri = text.slice(separator + 1) || undefined;
  }

  private remember(id: number, uri: string) {
    const previous = this.uris.get(id);
    if (previous) this.uriBytes -= previous.length;
    this.uris.delete(id);
    while (this.uris.size >= 4096 || this.uriBytes + uri.length > 4 * 1024 * 1024) {
      const oldest = this.uris.entries().next().value;
      if (!oldest) break;
      this.uris.delete(oldest[0]);
      this.uriBytes -= oldest[1].length;
    }
    this.uris.set(id, uri);
    this.uriBytes += uri.length;
  }
}
