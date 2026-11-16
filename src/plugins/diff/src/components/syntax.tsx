import { diffDiagnostic, useDiffRenderDiagnostic } from '../render-diagnostics';
import { useEffect, useState, useSyncExternalStore, type CSSProperties } from 'react';
import { Diff } from 'react-diff-view';
import { LanguageDescription } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { syntaxClient } from '../syntax-client';
import type { HunkData, HunkTokens } from 'react-diff-view';
import { useSdk } from '../../../../sdk';

/** Render plain hunks immediately and apply matching worker tokens when available. */
export function HighlightedDiff(
  props: React.ComponentProps<typeof Diff> & { path: string; patch: string },
) {
  const { path, patch, ...diffProps } = props;
  const sdk = useSdk();
  const theme = useSyncExternalStore(sdk.theme.subscribe, sdk.theme.getSnapshot).application;
  const language = LanguageDescription.matchFilename(languages, path.split('/').pop() ?? path);
  const [highlight, setHighlight] = useState<{
    path: string;
    patch: string;
    hunks: HunkData[];
    tokens: HunkTokens | undefined;
  }>();
  useEffect(() => {
    if (!language) return;
    let current = true;
    const request = syntaxClient.request(path, patch, diffProps.hunks);
    diffDiagnostic('highlight-request');
    void request.result.then((tokens) => {
      if (current) {
        setHighlight({ path, patch, hunks: diffProps.hunks, tokens });
        diffDiagnostic('highlight-ready');
      }
    });
    return () => {
      current = false;
      request.cancel();
    };
  }, [path, patch, diffProps.hunks, language]);
  const tokens =
    highlight?.path === path && highlight.patch === patch && highlight.hunks === diffProps.hunks
      ? highlight.tokens
      : undefined;
  useDiffRenderDiagnostic('HighlightedDiff', {
    hunks: diffProps.hunks,
    tokens,
    theme,
    widgets: diffProps.widgets,
  });
  const style = Object.fromEntries(
    Object.entries(theme.syntax).map(([key, color]) => [`--diff-syntax-${key}`, color]),
  ) as CSSProperties;
  return (
    <div style={style}>
      <Diff {...diffProps} tokens={tokens} />
    </div>
  );
}
