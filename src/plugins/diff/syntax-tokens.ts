import type { LanguageSupport } from '@codemirror/language';
import { highlightTree, tagHighlighter, tags } from '@lezer/highlight';
import type { HunkData, HunkTokens, TokenNode } from 'react-diff-view';

const highlighter = tagHighlighter([
  { tag: tags.keyword, class: 'diff-syntax-keyword' },
  { tag: [tags.string, tags.regexp], class: 'diff-syntax-string' },
  { tag: [tags.number, tags.bool, tags.null], class: 'diff-syntax-number' },
  { tag: tags.comment, class: 'diff-syntax-comment' },
  {
    tag: [tags.function(tags.variableName), tags.definition(tags.variableName)],
    class: 'diff-syntax-function',
  },
  { tag: [tags.typeName, tags.className], class: 'diff-syntax-type' },
  {
    tag: [tags.tagName, tags.attributeName, tags.propertyName, tags.operator],
    class: 'diff-syntax-property',
  },
  { tag: tags.invalid, class: 'diff-syntax-invalid' },
]);

// Parse each contiguous hunk independently: omitted source is unavailable, and
// allocating placeholder lines up to a large file's line numbers is unnecessary.
export function highlightHunks(hunks: HunkData[], support: LanguageSupport): HunkTokens {
  const result: HunkTokens = { old: [], new: [] };
  for (const hunk of hunks) {
    for (const side of ['old', 'new'] as const) {
      const changes = hunk.changes.filter(
        (change) => change.type !== (side === 'old' ? 'insert' : 'delete'),
      );
      const source = changes.map((change) => change.content).join('\n');
      const lines: TokenNode[][] = [[]];
      const append = (text: string, className?: string) => {
        text.split('\n').forEach((value, index) => {
          if (index) lines.push([]);
          if (value)
            lines[lines.length - 1].push(
              className
                ? { type: 'element', className, children: [{ type: 'text', value }] }
                : { type: 'text', value },
            );
        });
      };
      let cursor = 0;
      highlightTree(support.language.parser.parse(source), highlighter, (from, to, className) => {
        append(source.slice(cursor, from));
        append(source.slice(from, to), className);
        cursor = to;
      });
      append(source.slice(cursor));
      changes.forEach((change, index) => {
        const number =
          change.type === 'normal'
            ? side === 'old'
              ? change.oldLineNumber
              : change.newLineNumber
            : change.lineNumber;
        result[side][number - 1] = lines[index];
      });
    }
  }
  return result;
}
