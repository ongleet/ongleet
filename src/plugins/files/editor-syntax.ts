import { EditorView } from '@codemirror/view';
import type { ThemePreset } from '../../sdk';
import { HighlightStyle, LanguageDescription, syntaxHighlighting } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { tags } from '@lezer/highlight';

export function fileLanguage(path: string) {
  const filename = path.split('/').pop() ?? path;
  return LanguageDescription.matchFilename(languages, filename);
}

export const editorSyntaxHighlighting = (theme: ThemePreset) =>
  syntaxHighlighting(
    HighlightStyle.define([
      { tag: tags.keyword, color: theme.syntax.keyword },
      { tag: [tags.string, tags.regexp], color: theme.syntax.string },
      { tag: [tags.number, tags.bool, tags.null], color: theme.syntax.number },
      { tag: tags.comment, color: theme.syntax.comment, fontStyle: 'italic' },
      {
        tag: [tags.function(tags.variableName), tags.definition(tags.variableName)],
        color: theme.syntax.function,
      },
      { tag: [tags.typeName, tags.className], color: theme.syntax.type },
      { tag: [tags.tagName, tags.attributeName, tags.propertyName], color: theme.syntax.property },
      { tag: tags.operator, color: theme.syntax.property },
      { tag: tags.heading, color: theme.syntax.function, fontWeight: 'bold' },
      { tag: tags.link, color: theme.syntax.property, textDecoration: 'underline' },
      { tag: tags.emphasis, fontStyle: 'italic' },
      { tag: tags.strong, fontWeight: 'bold' },
      { tag: tags.invalid, color: theme.syntax.invalid },
    ]),
  );

export function editorAppearance(theme: ThemePreset) {
  return [EditorView.theme({}, { dark: theme.mode === 'dark' }), editorSyntaxHighlighting(theme)];
}
