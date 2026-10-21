// Shared by the live adapter and the WASM integration checks.
export const SCROLLBACK_LINES = 2000;
export const MAX_OUTPUT_CHUNK = 16 * 1024;

export function consumeOutput(terminal: { write(bytes: Uint8Array): void }, bytes: number[]) {
  if (bytes.length > MAX_OUTPUT_CHUNK) throw new Error('Output chunk exceeds the transport limit');
  terminal.write(Uint8Array.from(bytes));
}
