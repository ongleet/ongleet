import { SdkError } from './types';

export async function sendPtyText(
  write: (bytes: number[]) => Promise<void>,
  text: string,
  enter = false,
  bracketedPaste = false,
) {
  const bytes = new TextEncoder().encode(text);
  if (bytes.length + (enter ? 1 : 0) > 8192)
    throw new SdkError('limit_exceeded', 'Input exceeds 8 KiB including Enter');
  const input =
    bracketedPaste && bytes.length ? new TextEncoder().encode(`\x1b[200~${text}\x1b[201~`) : bytes;
  for (let offset = 0; offset < input.length; offset += 8192)
    await write(Array.from(input.subarray(offset, offset + 8192)));
  if (enter) {
    // PTY acknowledgement precedes TUI consumption. Leave room for large
    // multiline input to be processed and its paste suppression to expire.
    if (bytes.length) await new Promise((resolve) => setTimeout(resolve, 500));
    await write([13]);
  }
}
