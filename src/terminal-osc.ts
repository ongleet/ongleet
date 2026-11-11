// Compatibility boundary for ghostty-web 0.4.0: its OSC parser cannot answer
// default-color queries and warns on VTE/Codex notifications. Keep other bytes
// intact, including strings containing apparent escape sequences.
export class TerminalOsc {
  private state: 'ground' | 'escape' | 'csi' | 'osc' | 'oscEscape' | 'string' | 'stringEscape' =
    'ground';
  private pending: number[] = [];
  private overflow = false;
  private consume: (bytes: number[]) => void;
  private colors: () => { foreground: string; background: string };
  private reply: (text: string) => void;

  constructor(
    consume: (bytes: number[]) => void,
    colors: () => { foreground: string; background: string },
    reply: (text: string) => void,
  ) {
    this.consume = consume;
    this.colors = colors;
    this.reply = reply;
  }

  write(bytes: number[]) {
    let output: number[] = [];
    const flush = () => {
      if (output.length) this.consume(output);
      output = [];
    };
    const emit = (byte: number) => {
      output.push(byte);
      if (output.length === 16384) flush();
    };
    const release = () => {
      for (const byte of this.pending) emit(byte);
      this.pending = [];
    };
    for (const byte of bytes) {
      if (this.state === 'osc' || this.state === 'oscEscape') {
        if (this.overflow) emit(byte);
        else this.pending.push(byte);
        if (byte === 7 || (this.state === 'oscEscape' && byte === 92)) {
          const terminator = byte === 7 ? '\x07' : '\x1b\\';
          if (!this.overflow) {
            const payload = new TextDecoder().decode(
              new Uint8Array(this.pending.slice(2, -terminator.length)),
            );
            const query = /^(10|11);\?$/.exec(payload);
            if (query) {
              const color = this.colors()[query[1] === '10' ? 'foreground' : 'background'];
              if (/^#[0-9a-f]{6}$/i.test(color)) {
                flush();
                const rgb = color
                  .slice(1)
                  .match(/../g)!
                  .map((part) => part.repeat(2))
                  .join('/');
                this.reply(`\x1b]${query[1]};rgb:${rgb}${terminator}`);
                this.pending = [];
              } else release();
            } else if (/^(666;vte\.shell\.[\x20-\x7e]*|7501;[\x20-\x7e]*)$/.test(payload)) {
              this.pending = [];
            } else release();
          }
          this.state = 'ground';
        } else if (byte === 24 || byte === 26) {
          release();
          this.state = 'ground';
        } else {
          this.state = byte === 27 ? 'oscEscape' : 'osc';
          if (this.pending.length >= 8192) {
            release();
            this.overflow = true;
          }
        }
      } else if (this.state === 'escape') {
        if (byte === 93) {
          this.pending.push(byte);
          this.overflow = false;
          this.state = 'osc';
        } else {
          release();
          if (byte === 27) this.pending = [byte];
          else {
            emit(byte);
            this.state =
              byte === 91 ? 'csi' : [80, 88, 94, 95].includes(byte) ? 'string' : 'ground';
          }
        }
      } else {
        if ((this.state === 'ground' || this.state === 'csi') && byte === 27) {
          this.pending = [byte];
          this.state = 'escape';
          continue;
        }
        emit(byte);
        if (this.state === 'csi') {
          if ((byte >= 64 && byte <= 126) || byte === 24 || byte === 26) this.state = 'ground';
        } else if (this.state === 'string' || this.state === 'stringEscape') {
          if (byte === 24 || byte === 26 || (this.state === 'stringEscape' && byte === 92))
            this.state = 'ground';
          else this.state = byte === 27 ? 'stringEscape' : 'string';
        }
      }
    }
    flush();
  }
}
