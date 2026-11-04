import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
// Load the pure review model with its real public error class, without React's TSX entrypoint.
const types = ts.transpileModule(await readFile('src/sdk/types.ts', 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const typesUrl = `data:text/javascript;base64,${Buffer.from(types).toString('base64')}`;
const source = (await readFile('src/plugins/diff/review-state.ts', 'utf8')).replace(
  "'../../sdk'",
  JSON.stringify(typesUrl),
);
const model = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const { ReviewStore } = await import(
  `data:text/javascript;base64,${Buffer.from(model).toString('base64')}`
);
const store = new ReviewStore();
const files = [
  { path: 'a', key: 'a', patch: 'first', failed: false },
  { path: 'b', key: 'b', patch: undefined, failed: true },
];
store.reconcile('comparison', files);
const revision = store.revision;
assert.throws(() => store.set(['a', 'missing'], 'expanded', false), /Unknown/);
assert.equal(store.get('a').expanded, true);
assert.throws(() => store.set(['a', 'b'], 'viewed', true, revision), /unavailable/);
assert.equal(store.get('a').viewed, false);
store.set(['a'], 'viewed', true, revision);
assert.deepEqual(store.get('a'), { viewed: true, expanded: false });
store.set(['a'], 'expanded', true);
assert.deepEqual(store.get('a'), { viewed: true, expanded: true });
store.reconcile(
  'comparison',
  files.map((f) => ({ ...f })),
);
assert.equal(store.revision, revision);
assert.equal(store.get('a').viewed, true);
store.reconcile('comparison', [{ ...files[0], patch: 'new content' }, files[1]]);
assert.notEqual(store.revision, revision);
assert.deepEqual(store.get('a'), { viewed: false, expanded: true });
assert.throws(() => store.set(['a'], 'viewed', true, revision), /Comparison changed/);
store.set(['a'], 'viewed', true, store.revision);
store.reconcile('another comparison', [{ ...files[0], patch: 'new content' }]);
assert.equal(store.get('a').viewed, false);
console.log('Diff interaction batch validation, revision and reset checks passed.');

store.set(['a'], 'expanded', false);
store.reconcile('another comparison', [{ ...files[0], patch: 'changed but unviewed' }]);
assert.deepEqual(store.get('a'), { viewed: false, expanded: false });
