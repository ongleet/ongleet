import assert from 'node:assert/strict';
import { languages } from '@codemirror/language-data';
import { LanguageDescription } from '@codemirror/language';
import { parseDiff } from 'react-diff-view';
import { highlightHunks } from '../src/plugins/diff/syntax-tokens.ts';

const support = await LanguageDescription.matchFilename(languages, 'example.ts').load();
const [{ hunks }] = parseDiff(`diff --git a/example.ts b/example.ts
--- a/example.ts
+++ b/example.ts
@@ -10,4 +10,4 @@
 /* comment
  * continued */
-const before = "old";
+const after = 42;
 console.log("<tag>&");
@@ -100000,1 +100000,1 @@
-return false;
+return true;
`);
const tokens = highlightHunks(hunks, support);
const text = (nodes) =>
  nodes.map((node) => (node.type === 'text' ? node.value : text(node.children))).join('');
const colored = (nodes, className) =>
  nodes
    .filter((node) => node.className === className)
    .map((node) => text(node.children))
    .join('');
assert.equal(text(tokens.old[11]), 'const before = "old";');
assert.equal(text(tokens.new[11]), 'const after = 42;');
assert.equal(colored(tokens.old[11], 'diff-syntax-string'), '"old"');
assert.equal(colored(tokens.new[11], 'diff-syntax-number'), '42');
assert.equal(colored(tokens.new[10], 'diff-syntax-comment'), ' * continued */');
assert.equal(text(tokens.new[12]), 'console.log("<tag>&");');
assert.equal(text(tokens.new[99999]), 'return true;');
assert.equal(tokens.new[99998], undefined);
console.log('Diff syntax preserves text, side/line mapping, multiline comments and hunk gaps.');
