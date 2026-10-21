// Shared by the live adapter and the WASM integration checks.
export const SCROLLBACK_LINES = 2000;
export const MAX_OUTPUT_CHUNK = 16 * 1024;

export function consumeOutput(terminal: { write(bytes: Uint8Array): void }, bytes: number[]) {
  if (bytes.length > MAX_OUTPUT_CHUNK) throw new Error('Output chunk exceeds the transport limit');
  terminal.write(Uint8Array.from(bytes));
}

// ghostty-web 0.4.0 can expose stale cells when a freed WASM allocation is
// reused. Reset each new emulator before attaching output, including replay.
export function initializeTerminal(terminal: { write(bytes: Uint8Array): void }) {
  terminal.write(Uint8Array.of(0x1b, 0x63)); // RIS: fresh screen and default VT modes.
}
