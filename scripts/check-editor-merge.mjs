import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mergeText } from '../src/plugins/files/editor-merge.ts';

test('applies independent local edits onto the new disk contents', () => {
  assert.deepEqual(
    mergeText(
      'Title\nkeep\nBody\nFooter\n',
      'New title\nkeep\nBody\nFooter\n',
      'Title\nkeep\nNew body\nFooter\n',
    ),
    { text: 'New title\nkeep\nNew body\nFooter\n', conflict: false },
  );
});

test('keeps competing edits and their original context in conflict markers', () => {
  assert.deepEqual(mergeText('start\noriginal\nend', 'start\nlocal\nend', 'start\ndisk\nend'), {
    text: 'start\n<<<<<<< Local edits\nlocal\n||||||| Original\noriginal\n=======\ndisk\n>>>>>>> Disk version\nend',
    conflict: true,
  });
});

test('does not conflict when both sides made the same edit', () => {
  assert.deepEqual(mergeText('before\n', 'after\n', 'after\n'), {
    text: 'after\n',
    conflict: false,
  });
});

test('preserves whitespace, Unicode and a missing trailing newline', () => {
  assert.deepEqual(mergeText('a\nkeep\nz', '  café\t\nkeep\nz', 'a\nkeep\n日本語'), {
    text: '  café\t\nkeep\n日本語',
    conflict: false,
  });
});

test('preserves deletions and independent insertions', () => {
  assert.deepEqual(mergeText('a\nremove\nb\nc\n', 'a\nb\nc\n', 'a\nremove\nb\nc\nadded\n'), {
    text: 'a\nb\nc\nadded\n',
    conflict: false,
  });
});

test('handles empty original content without losing either competing insertion', () => {
  const merged = mergeText('', 'local', 'disk');
  assert.equal(merged.conflict, true);
  assert.ok(merged.text.includes('local'));
  assert.ok(merged.text.includes('disk'));
});

test('bounds repetitive diff work rather than exhausting resources', () => {
  const repeated = 'same\n'.repeat(2000);
  assert.throws(
    () => mergeText(repeated, `local\n${repeated}`, `disk\n${repeated}`),
    /too complex/,
  );
});
