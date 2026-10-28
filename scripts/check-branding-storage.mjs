import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

const source = ts.transpileModule(readFileSync('src/branding-storage.ts', 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
function fixture(entries, failKey) {
  const values = new Map(entries);
  const storage = {
    get length() {
      return values.size;
    },
    key: (index) => [...values.keys()][index] ?? null,
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => {
      if (key === failKey) throw new Error('Quota exceeded');
      values.set(key, value);
    },
    removeItem: (key) => values.delete(key),
  };
  return { values, storage };
}
function run(storage) {
  vm.runInNewContext(source, {
    exports: {},
    window: { localStorage: storage },
    document: { body: { textContent: '' } },
  });
}
const original = [
  ['devcenter.editor.wrap', 'true'],
  ['devcenter.editor.draft:[7,"/tmp/file"]', 'unsaved café'],
  ['devcenter.pr-reviews.v1:7:repo:viewer', 'acknowledged'],
  ['unrelated', 'devcenter is literal content'],
];
const interrupted = fixture(original, 'ongleet.pr-reviews.v1:7:repo:viewer');
assert.throws(() => run(interrupted.storage), /Quota exceeded/);
for (const [key, value] of original) assert.equal(interrupted.values.get(key), value);
const resumed = fixture(interrupted.values);
run(resumed.storage);
assert.equal(resumed.values.get('ongleet.editor.draft:[7,"/tmp/file"]'), 'unsaved café');
assert.equal(resumed.values.get('ongleet.editor.wrap'), 'true');
assert.equal(resumed.values.get('ongleet.pr-reviews.v1:7:repo:viewer'), 'acknowledged');
assert.equal(resumed.values.get('unrelated'), 'devcenter is literal content');
assert.equal(resumed.values.has('devcenter.editor.wrap'), false);
const completed = [...resumed.values];
run(resumed.storage);
assert.deepEqual([...resumed.values], completed);
const conflict = fixture([...original, ['ongleet.editor.wrap', 'false']]);
assert.throws(() => run(conflict.storage), /Conflicting Ongleet browser storage/);
assert.deepEqual([...conflict.values], [...original, ['ongleet.editor.wrap', 'false']]);
console.log('Browser branding migration preserves drafts on interruption and conflicts.');

assert.doesNotThrow(() =>
  vm.runInNewContext(source, {
    exports: {},
    window: {
      get localStorage() {
        throw new Error('Storage denied');
      },
    },
    document: { body: { textContent: '' } },
  }),
);
