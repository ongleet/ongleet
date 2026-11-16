import { LanguageDescription } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import type { HunkData, HunkTokens } from 'react-diff-view';
import { highlightHunks } from './syntax-tokens.ts';

export type HighlightRequest = { id: number; filename: string; hunks: HunkData[] };
export type HighlightResponse = { id: number; tokens?: HunkTokens };

self.onmessage = async ({ data }: MessageEvent<HighlightRequest>) => {
  let tokens: HunkTokens | undefined;
  try {
    const language = LanguageDescription.matchFilename(
      languages,
      data.filename.split('/').pop() ?? data.filename,
    );
    if (language) tokens = highlightHunks(data.hunks, await language.load());
  } catch {
    // Unsupported languages and parser failures retain the plain patch.
  }
  self.postMessage({ id: data.id, tokens } satisfies HighlightResponse);
};
