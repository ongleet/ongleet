import assert from 'node:assert/strict';
import { mock } from 'node:test';
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
    record = { version: 2, revision: revision + 1, value };
    return record;
  },
};
const note = new NoteModel(storage, () => {});
await note.load();
assert.equal(note.notes.length, 1);
assert.equal(note.getNote().name, 'Note 1');
const firstId = note.selectedNoteId;
assert.equal(record.version, 2);
note.edit('# TODO\n- [ ] First');
release = true;
const saving = note.save();
await Promise.resolve();
note.edit('# TODO\n- [x] First\n- [ ] Next');
const finish = release;
release = null;
finish();
await saving;
assert.equal(
  record.value.notes.find((item) => item.id === firstId).text,
  '# TODO\n- [x] First\n- [ ] Next',
);
assert.equal(note.dirty, false);
const reopened = new NoteModel(storage, () => {});
await reopened.load();
assert.equal(reopened.text, '# TODO\n- [x] First\n- [ ] Next');
const signal = new AbortController().signal;
await assert.rejects(note.replace('stale overwrite', 0, signal), /Read Note state again/);
await note.replace('Agent memory', note.revision, signal);
assert.equal(record.value.notes.find((item) => item.id === firstId).text, 'Agent memory');
note.edit('Retain on failure');
fail = true;
await assert.rejects(note.save(), /disk full/);
assert.equal(note.text, 'Retain on failure');
assert.equal(note.dirty, true);
await assert.rejects(note.replace('overwrite draft', note.revision, signal), /unsaved edits/);
fail = false;
await note.save();
assert.equal(record.value.notes.find((item) => item.id === firstId).text, 'Retain on failure');
// A concurrent writer must never be silently overwritten; reloading is explicit.
record = {
  ...record,
  revision: record.revision + 1,
  value: { ...record.value, notes: [{ id: firstId, name: 'Note 1', text: 'Other writer' }] },
};
note.edit('Stale draft');
await assert.rejects(note.save(), /revision_conflict/);
assert.equal(record.value.notes.find((item) => item.id === firstId).text, 'Other writer');
assert.equal(note.text, 'Stale draft');
await note.load(true);
assert.equal(note.text, 'Other writer');
assert.equal(note.dirty, false);
assert.throws(() => note.edit('日'.repeat(8193)), /24 KiB/);
assert.throws(() => note.edit('\u0000'.repeat(10000)), /encoded storage limit/);
const abort = new AbortController();
abort.abort();
await assert.rejects(note.replace('cancelled', note.revision, abort.signal));
assert.equal(record.value.notes.find((item) => item.id === firstId).text, 'Other writer');
// Names, selection and per-note content survive restart, including queued writes.
const secondId = note.add('Agent memory');
note.edit('Remember the failing test');
release = true;
const notebookSave = note.save();
await Promise.resolve();
note.select(firstId);
note.rename('Session TODO');
note.edit('- [ ] Review implementation');
const finishNotebook = release;
release = null;
finishNotebook();
await notebookSave;
const notebook = new NoteModel(storage, () => {});
await notebook.load();
assert.equal(notebook.selectedNoteId, firstId);
assert.equal(notebook.getNote().name, 'Session TODO');
assert.equal(notebook.text, '- [ ] Review implementation');
assert.equal(notebook.getNote(secondId).name, 'Agent memory');
assert.equal(notebook.getNote(secondId).text, 'Remember the failing test');
await notebook.replace('Updated by agent', notebook.revision, signal, secondId);
assert.equal(notebook.selectedNoteId, firstId);
assert.equal(notebook.text, '- [ ] Review implementation');
assert.equal(notebook.getNote(secondId).text, 'Updated by agent');
assert.throws(() => notebook.rename('  '), /Note names/);
assert.throws(() => notebook.rename('日'.repeat(43)), /Note names/);
assert.throws(() => notebook.select('missing'), /Unknown note/);
assert.throws(() => notebook.edit('oops', 'missing'), /Unknown note/);
// A failed aggregate-limit update must leave the original notebook intact.
notebook.edit('x'.repeat(24000));
await notebook.save();
notebook.select(secondId);
notebook.edit('y'.repeat(24000));
await notebook.save();
notebook.add('Third');
const beforeLimit = JSON.stringify(notebook.book);
assert.throws(() => notebook.edit('z'.repeat(4000)), /encoded storage limit/);
assert.equal(JSON.stringify(notebook.book), beforeLimit);
await notebook.save();
// Existing version-1 notes migrate once, preserving all text and stable IDs.
record = { version: 1, revision: 7, value: { text: '# Existing memory\n- Keep this' } };
const migrated = new NoteModel(storage, () => {});
await migrated.load();
assert.equal(migrated.notes.length, 1);
assert.equal(migrated.text, '# Existing memory\n- Keep this');
assert.equal(record.version, 2);
assert.equal(record.revision, 8);
const migratedId = migrated.selectedNoteId;
await migrated.load();
assert.equal(migrated.selectedNoteId, migratedId);
assert.equal(record.revision, 8);
// Typing resets the debounce; explicit lifecycle saves flush without waiting.
mock.timers.enable({ apis: ['setTimeout'] });
try {
  migrated.edit('First keystrokes');
  migrated.scheduleSave();
  mock.timers.tick(400);
  migrated.edit('Latest keystrokes');
  migrated.scheduleSave();
  mock.timers.tick(499);
  await Promise.resolve();
  assert.equal(record.revision, 8);
  mock.timers.tick(1);
  await migrated.save();
  assert.equal(record.revision, 9);
  assert.equal(record.value.notes[0].text, 'Latest keystrokes');
  migrated.edit('Flush before quit');
  migrated.scheduleSave();
  await migrated.save();
  assert.equal(record.value.notes[0].text, 'Flush before quit');
  mock.timers.tick(500);
  await Promise.resolve();
  assert.equal(record.revision, 10);
  migrated.edit('Discard this draft');
  migrated.scheduleSave();
  await migrated.load(true);
  mock.timers.tick(500);
  await Promise.resolve();
  assert.equal(migrated.text, 'Flush before quit');
  assert.equal(record.revision, 10);
} finally {
  mock.timers.reset();
}
console.log(
  'Notebook migration, multiple notes, queued autosave, recovery and revision checks passed.',
);
