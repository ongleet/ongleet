import { HighlightStyle, LanguageDescription, syntaxHighlighting } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { tags } from '@lezer/highlight';

export function fileLanguage(path: string) {
  const filename = path.split('/').pop() ?? path;
  return LanguageDescription.matchFilename(languages, filename);
}

export const editorSyntaxHighlighting = syntaxHighlighting(
  HighlightStyle.define([
    { tag: tags.keyword, color: '#c792ea' },
    { tag: [tags.string, tags.regexp], color: '#c3e88d' },
    { tag: [tags.number, tags.bool, tags.null], color: '#f78c6c' },
    { tag: tags.comment, color: '#8798af', fontStyle: 'italic' },
    {
      tag: [tags.function(tags.variableName), tags.definition(tags.variableName)],
      color: '#82aaff',
    },
    { tag: [tags.typeName, tags.className], color: '#ffcb6b' },
    { tag: [tags.tagName, tags.attributeName, tags.propertyName], color: '#89ddff' },
    { tag: tags.operator, color: '#89ddff' },
    { tag: tags.heading, color: '#82aaff', fontWeight: 'bold' },
    { tag: tags.link, color: '#89ddff', textDecoration: 'underline' },
    { tag: tags.emphasis, fontStyle: 'italic' },
    { tag: tags.strong, fontWeight: 'bold' },
    { tag: tags.invalid, color: '#ff6b7a' },
  ]),
);
