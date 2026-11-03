import { useEffect, useMemo, useState, useSyncExternalStore, type CSSProperties } from 'react';
import { LanguageDescription, type LanguageSupport } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { Diff } from 'react-diff-view';
import { highlightHunks } from './syntax-tokens';
import { useSdk } from '../../sdk';

export function HighlightedDiff(props: React.ComponentProps<typeof Diff> & { path: string }) {
  const { path, ...diffProps } = props;
  const sdk = useSdk();
  const theme = useSyncExternalStore(sdk.theme.subscribe, sdk.theme.getSnapshot).application;
  const language = LanguageDescription.matchFilename(languages, path.split('/').pop() ?? path);
  const [loaded, setLoaded] = useState<{ language: typeof language; support: LanguageSupport }>();
  useEffect(() => {
    let current = true;
    if (language)
      void language
        .load()
        .then((support) => {
          if (current) setLoaded({ language, support });
        })
        .catch(() => {
          /* Keep plain text when a language cannot be loaded. */
        });
    return () => {
      current = false;
    };
  }, [language]);
  const tokens = useMemo(() => {
    if (!loaded || loaded.language !== language) return undefined;
    try {
      return highlightHunks(diffProps.hunks, loaded.support);
    } catch {
      return undefined;
    }
  }, [diffProps.hunks, loaded, language]);
  const style = Object.fromEntries(
    Object.entries(theme.syntax).map(([key, color]) => [`--diff-syntax-${key}`, color]),
  ) as CSSProperties;
  return (
    <div style={style}>
      <Diff {...diffProps} tokens={tokens} />
    </div>
  );
}
