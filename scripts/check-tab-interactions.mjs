import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
// Load the pure review model with its real public error class, without React's TSX entrypoint.
const types = ts.transpileModule(await readFile('src/sdk/types.ts', 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const typesUrl = `data:text/javascript;base64,${Buffer.from(types).toString('base64')}`;
const source = (await readFile('src/plugins/diff/src/review-state.ts', 'utf8')).replace(
  "'../../../sdk'",
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

const large = new ReviewStore();
const smallPatch = Array.from({ length: 500 }, () => '+x').join('\n');
const hugePatch = smallPatch + '\n+x';
const fixture = (patch) => [{ path: 'large.ts', key: 'large', patch, failed: false }];
large.reconcile('same', fixture(smallPatch));
assert.equal(large.get('large').expanded, true, '500 lines remain open');
large.reconcile('same', fixture(hugePatch));
assert.deepEqual(
  large.get('large'),
  { expanded: false, viewed: false },
  'newly large patches close without marking viewed',
);
large.set(['large.ts'], 'expanded', true);
large.reconcile('same', fixture(hugePatch + '\n+changed'));
assert.equal(large.get('large').expanded, true, 'manual expansion survives refresh and edits');
large.reconcile('other', fixture(hugePatch));
assert.equal(large.get('large').expanded, false, 'new comparison resets manual expansion');
large.reconcile('bytes', fixture('x'.repeat(65536)));
assert.equal(large.get('large').expanded, true, '64 KiB remains open');
large.reconcile('bytes', fixture('x'.repeat(65537)));
assert.equal(large.get('large').expanded, false, 'large single lines start closed');
large.reconcile('unicode', fixture('é'.repeat(32769)));
assert.equal(large.get('large').expanded, false, 'size threshold uses UTF-8 bytes');
large.set(['large.ts'], 'viewed', true, large.revision);
large.reconcile('unicode', fixture(hugePatch));
assert.deepEqual(
  large.get('large'),
  { expanded: false, viewed: false },
  'changed large viewed patches clear the mark and retain large-file default',
);
large.reconcile('unicode', fixture('small again'));
assert.equal(
  large.get('large').expanded,
  true,
  'automatic reset after a viewed patch changes must not become a manual collapse',
);
console.log('Large Diff defaults, UTF-8 bounds and manual expansion checks passed.');

// Many small patches must not mount an unbounded number of code tables.
const many = new ReviewStore();
const manyFiles = Array.from({ length: 1000 }, (_, i) => ({
  path: `file-${i}.ts`,
  key: `file-${i}`,
  patch: '+small change\n',
  failed: false,
}));
many.reconcile('many', manyFiles);
assert.equal(manyFiles.filter((file) => many.get(file.key).expanded).length, 20);
many.set(['file-999.ts'], 'expanded', true);
many.set(['file-0.ts'], 'expanded', false);
many.reconcile(
  'many',
  manyFiles.map((file) => ({ ...file, patch: '+updated\n' })),
);
assert.equal(many.get('file-999').expanded, true);
assert.equal(many.get('file-0').expanded, false);
assert.equal(manyFiles.filter((file) => many.get(file.key).expanded).length, 21);
many.reconcile('different', manyFiles);
assert.equal(many.get('file-999').expanded, false);
assert.equal(many.get('file-0').expanded, true);
