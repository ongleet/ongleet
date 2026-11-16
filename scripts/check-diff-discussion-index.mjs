import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseDiff } from 'react-diff-view';
import { DiscussionIndex, LineDiscussionIndex } from '../src/plugins/diff/src/discussion-index.ts';

function thread(id, path, side, line, orphan = false) {
  const location = { root: '/repo', path, side, line, context: [] };
  return {
    id,
    original: location,
    location,
    orphan,
    comments: [{ id: `comment-${id}`, text: id, origin: 'User', at: '2026-11-16T10:00:00Z' }],
  };
}

test('indexes file and line discussions separately and retains per-file revisions', () => {
  const file = thread('file', 'a.ts');
  const line = thread('line', 'a.ts', 'new', 4);
  const other = thread('other', 'b.ts', 'old', 4);
  const orphan = thread('orphan', 'a.ts', 'new', 4, true);
  const snapshot = [file, line, other, orphan];
  const index = new DiscussionIndex().read(snapshot);
  const a = index.forFile('a.ts'),
    b = index.forFile('b.ts');
  assert.deepEqual(a.file, [file]);
  assert.deepEqual(a.line, [line]);
  assert.deepEqual(index.orphans, [orphan]);
  index.read(snapshot);
  assert.equal(index.forFile('a.ts'), a);
  index.read([
    file,
    { ...line, comments: [...line.comments, { ...line.comments[0], id: 'reply', text: 'reply' }] },
    other,
    orphan,
  ]);
  assert.notEqual(index.forFile('a.ts').line, a.line);
  assert.equal(index.forFile('a.ts').file, a.file);
  assert.equal(index.forFile('b.ts'), b);
  const lines = index.forFile('a.ts').line;
  index.read([
    { ...file, comments: [{ ...file.comments[0], text: 'edited file comment' }] },
    ...lines,
    other,
    orphan,
  ]);
  assert.equal(
    index.forFile('a.ts').line,
    lines,
    'file-level edits must not invalidate line widgets',
  );
});

test('moves, orphans and deletions remove old buckets without dropping other threads', () => {
  const moving = thread('moving', 'a.ts', 'new', 8),
    staying = thread('staying', 'b.ts');
  const index = new DiscussionIndex().read([moving, staying]);
  const b = index.forFile('b.ts');
  const renamed = { ...moving, location: { ...moving.location, path: 'renamed.ts', line: 12 } };
  index.read([renamed, staying]);
  assert.deepEqual(index.forFile('a.ts').line, []);
  assert.equal(index.forFile('renamed.ts').line[0].location.line, 12);
  index.read([{ ...renamed, orphan: true }, staying]);
  assert.deepEqual(index.forFile('renamed.ts').line, []);
  assert.equal(index.orphans[0].id, 'moving');
  index.read([renamed, staying]);
  assert.deepEqual(index.orphans, []);
  index.read([staying]);
  assert.deepEqual(index.forFile('renamed.ts').line, []);
  assert.equal(index.forFile('b.ts'), b);
});

test('context rows merge both sides in original order; changed rows use only their side', () => {
  const [file] = parseDiff(
    'diff --git a/a.ts b/a.ts\n--- a/a.ts\n+++ b/a.ts\n@@ -10,3 +20,3 @@\n context\n-old\n+new\n end\n',
  );
  const items = [
    { id: 'new-first', side: 'new', line: 20 },
    { id: 'old-second', side: 'old', line: 10 },
    { id: 'new-third', side: 'new', line: 20 },
    { id: 'deleted', side: 'old', line: 11 },
    { id: 'inserted', side: 'new', line: 21 },
    { id: 'wrong-insert-side', side: 'old', line: 21 },
    { id: 'wrong-delete-side', side: 'new', line: 11 },
  ];
  const index = new LineDiscussionIndex(items);
  const changes = file.hunks[0].changes;
  assert.deepEqual(
    index.forChange(changes[0]).map((item) => item.id),
    ['new-first', 'old-second', 'new-third'],
  );
  assert.deepEqual(
    index.forChange(changes[1]).map((item) => item.id),
    ['deleted'],
  );
  assert.deepEqual(
    index.forChange(changes[2]).map((item) => item.id),
    ['inserted'],
  );
  assert.deepEqual(index.forChange(changes[3]), []);
  const composer = { id: 'composer', side: 'new', line: 20 };
  assert.deepEqual(
    new LineDiscussionIndex([...items, composer]).forChange(changes[0]).map((item) => item.id),
    ['new-first', 'old-second', 'new-third', 'composer'],
  );
});
