import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText).toString('base64')}`;
const types = moduleUrl(await readFile('src/sdk/types.ts', 'utf8'));
const { NoteModel } = await import(
  moduleUrl(
    (await readFile('src/plugins/note/model.ts', 'utf8')).replaceAll(
      "'../../sdk'",
      JSON.stringify(types),
    ),
  )
);
let record = null;
let release;
let fail = false;
const storage = {
  read: async () => record,
  replace: async (revision, value) => {
    if (release)
      await new Promise((resolve) => {
        release = resolve;
      });
    if (fail) throw new Error('disk full');
    if (revision !== (record?.revision ?? 0)) throw new Error('revision_conflict');
    record = { version: 1, revision: revision + 1, value };
    return record;
  },
};
const note = new NoteModel(storage, () => {});
await note.load();
note.edit('# TODO\n- [ ] First');
release = true;
const saving = note.save();
await Promise.resolve();
note.edit('# TODO\n- [x] First\n- [ ] Next');
const finish = release;
release = null;
finish();
await saving;
assert.equal(record.value.text, '# TODO\n- [x] First\n- [ ] Next');
assert.equal(note.dirty, false);
const reopened = new NoteModel(storage, () => {});
await reopened.load();
assert.equal(reopened.text, '# TODO\n- [x] First\n- [ ] Next');
const signal = new AbortController().signal;
await assert.rejects(note.replace('stale overwrite', 0, signal), /Read Note state again/);
await note.replace('Agent memory', note.revision, signal);
assert.equal(record.value.text, 'Agent memory');
note.edit('Retain on failure');
fail = true;
await assert.rejects(note.save(), /disk full/);
assert.equal(note.text, 'Retain on failure');
assert.equal(note.dirty, true);
await assert.rejects(note.replace('overwrite draft', note.revision, signal), /unsaved edits/);
fail = false;
await note.save();
assert.equal(record.value.text, 'Retain on failure');
// A concurrent writer must never be silently overwritten; reloading is explicit.
record = { ...record, revision: record.revision + 1, value: { text: 'Other writer' } };
note.edit('Stale draft');
await assert.rejects(note.save(), /revision_conflict/);
assert.equal(record.value.text, 'Other writer');
assert.equal(note.text, 'Stale draft');
await note.load(true);
assert.equal(note.text, 'Other writer');
assert.equal(note.dirty, false);
assert.throws(() => note.edit('日'.repeat(8193)), /24 KiB/);
assert.throws(() => note.edit('\u0000'.repeat(10000)), /encoded storage limit/);
const abort = new AbortController();
abort.abort();
await assert.rejects(note.replace('cancelled', note.revision, abort.signal));
assert.equal(record.value.text, 'Other writer');
console.log('Note autosave, recovery, revision protection and bounds passed.');
