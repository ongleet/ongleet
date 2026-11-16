import { mergeDiff3 } from 'node-diff3';

export type MergeResult = { text: string; conflict: boolean };

export function mergeText(original: string, local: string, disk: string): MergeResult {
  if (local === disk || disk === original) return { text: local, conflict: false };
  if (local === original) return { text: disk, conflict: false };
  const inputs = [original, local, disk];
  const lines = inputs.map((text) => text.split('\n'));
  if (
    inputs.some((text) => text.length > 1024 * 1024) ||
    lines.some((part) => part.length > 20000)
  ) {
    throw new Error('File is too large to merge automatically. Your edits have been kept.');
  }
  // Bound the matching-pair work of the library's LCS algorithm, including repeated lines.
  const occurrences = new Map<string, number>();
  for (const line of lines[0]) occurrences.set(line, (occurrences.get(line) ?? 0) + 1);
  let pairs = 0;
  for (const line of [...lines[1], ...lines[2]]) pairs += occurrences.get(line) ?? 0;
  if (pairs > 2_000_000) {
    throw new Error('Changes are too complex to merge automatically. Your edits have been kept.');
  }
  const merged = mergeDiff3(lines[1], lines[0], lines[2], {
    excludeFalseConflicts: true,
    label: { a: 'Local edits', o: 'Original', b: 'Disk version' },
  });
  return { text: merged.result.join('\n'), conflict: merged.conflict };
}

export function startMerge(original: string, local: string, disk: string) {
  const worker = new Worker(new URL('./merge-worker.ts', import.meta.url), { type: 'module' });
  let cancel = () => {};
  const result = new Promise<MergeResult>((resolve, reject) => {
    const finish = (error?: Error, value?: MergeResult) => {
      window.clearTimeout(timer);
      worker.terminate();
      if (error) reject(error);
      else resolve(value!);
    };
    const timer = window.setTimeout(
      () => finish(new Error('Merge timed out. Your edits have been kept.')),
      5000,
    );
    cancel = () => finish(new Error('Merge cancelled.'));
    worker.onerror = () => finish(new Error('Could not merge changes. Your edits have been kept.'));
    worker.onmessage = (event: MessageEvent<{ result?: MergeResult; error?: string }>) => {
      if (event.data.error) finish(new Error(event.data.error));
      else finish(undefined, event.data.result);
    };
    worker.postMessage({ original, local, disk });
  });
  return { result, cancel };
}
