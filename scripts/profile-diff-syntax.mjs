import assert from 'node:assert/strict';
import { performance } from 'node:perf_hooks';
import { LanguageDescription } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { parseDiff } from 'react-diff-view';
import { highlightHunks } from '../src/plugins/diff/src/syntax-tokens.ts';
import { SyntaxClient } from '../src/plugins/diff/src/syntax-client.ts';
import { ThreadPort } from './diff-syntax-worker-port.mjs';

// Synthetic parser/transport comparison, not a WebKitGTK rendering benchmark.
const support = await LanguageDescription.matchFilename(languages, 'example.ts').load();
const lines = Array.from(
  { length: 200 },
  (_, i) => `const value${i} = { count: ${i}, label: "example", active: true };`,
);
const patch = [
  'diff --git a/example.ts b/example.ts',
  '--- a/example.ts',
  '+++ b/example.ts',
  '@@ -1,200 +1,200 @@',
  ...lines.map((line) => '-' + line),
  ...lines.map((line) => '+' + line.replace('example', 'updated')),
  '',
].join('\n');
const [{ hunks }] = parseDiff(patch);
async function measure(work) {
  let last = performance.now(),
    maxGap = 0;
  const timer = setInterval(() => {
    const now = performance.now();
    maxGap = Math.max(maxGap, now - last);
    last = now;
  }, 5);
  try {
    await new Promise((resolve) => setTimeout(resolve, 15));
    const start = performance.now();
    await work();
    const elapsed = performance.now() - start;
    await new Promise((resolve) => setTimeout(resolve, 15));
    return { elapsedMs: Math.round(elapsed), maxTimerGapMs: Math.round(maxGap) };
  } finally {
    clearInterval(timer);
  }
}
const synchronous = await measure(() => {
  for (let i = 0; i < 20; i++) highlightHunks(hunks, support);
});
const client = new SyntaxClient(() => new ThreadPort());
const worker = await measure(async () => {
  const results = await Promise.all(
    Array.from({ length: 20 }, (_, i) => client.request(`example${i}.ts`, patch, hunks).result),
  );
  assert.ok(
    results.every((tokens) => tokens?.new?.[0]?.length),
    'Every worker request must produce tokens',
  );
});
console.log(
  JSON.stringify(
    { scenario: '20 TypeScript patches, 400 changed lines each; 5ms timer', synchronous, worker },
    null,
    2,
  ),
);
