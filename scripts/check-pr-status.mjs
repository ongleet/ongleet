import assert from 'node:assert/strict';
import { test } from 'node:test';
import { updatePullRequestTag } from '../src/plugins/pull-request/status-tag.ts';
import { refreshSeconds } from '../src/plugins/pull-request/settings.ts';
import {
  otherReviews,
  unreadReviews,
  acknowledgeReviews,
} from '../src/plugins/pull-request/reviews.ts';

test('existing configurations default to two minutes and unsafe timer values are rejected', () => {
  assert.equal(refreshSeconds({}), 120);
  assert.equal(refreshSeconds({ refreshIntervalSeconds: 300 }), 300);
  for (const seconds of [0, -1, 29, 86401, 30.5, '120', null])
    assert.throws(() => refreshSeconds({ refreshIntervalSeconds: seconds }), /Refresh interval/);
});

const review = (id, login, submittedAt, state = 'APPROVED') => ({
  id,
  author: login ? { login } : null,
  submittedAt,
  state,
});

test('baseline excludes own, pending and deleted-author reviews; new submitted reviews stay unread', () => {
  const existing = review('old', 'colleague', '2026-10-25T10:00:00Z');
  const all = [
    existing,
    review('self', 'ME', '2026-10-25T10:05:00Z'),
    review('draft', 'colleague', null, 'PENDING'),
    review('deleted', null, '2026-10-25T10:06:00Z'),
  ];
  const baseline = otherReviews(all, 'me');
  assert.deepEqual(baseline, [existing]);
  const saved = JSON.parse(JSON.stringify(acknowledgeReviews(baseline)));
  assert.deepEqual(unreadReviews(baseline, saved), []);
  const added = review('new', 'reviewer', '2026-10-25T10:10:00Z', 'CHANGES_REQUESTED');
  const refreshed = otherReviews([...all, added], 'me');
  assert.deepEqual(unreadReviews(refreshed, saved), [added]);
  assert.deepEqual(unreadReviews(refreshed, saved), [added]);
  const read = acknowledgeReviews(refreshed, saved);
  assert.deepEqual(unreadReviews(refreshed, read), []);
  assert.deepEqual(acknowledgeReviews([existing], read), read);
});

test('a second review at the same timestamp is new; empty baselines detect the first review', () => {
  const first = review('one', 'alice', '2026-10-25T10:00:00Z');
  const second = review('two', 'bob', '2026-10-25T10:00:00Z', 'COMMENTED');
  assert.deepEqual(unreadReviews([first], acknowledgeReviews([])), [first]);
  const cursor = acknowledgeReviews([first]);
  assert.deepEqual(unreadReviews([first, second], cursor), [second]);
  assert.deepEqual(unreadReviews([first, second], acknowledgeReviews([second], cursor)), []);
});

test('PR transitions and removal preserve sibling tags and avoid redundant writes', async () => {
  const sibling = { name: 'Codex', dot: '#123456', priority: 10 };
  let tags = [sibling];
  let writes = 0;
  const service = {
    async listTags() {
      return tags;
    },
    async addTag(tag) {
      writes++;
      tags = [...tags.filter((item) => item.name !== tag.name), tag];
      return tags;
    },
    async removeTag(name) {
      writes++;
      tags = tags.filter((tag) => tag.name !== name);
      return tags;
    },
  };
  for (const state of ['OPEN', 'CLOSED', 'MERGED']) {
    await updatePullRequestTag(service, state);
    assert.deepEqual(tags, [
      sibling,
      {
        name: `PR: ${state[0]}${state.slice(1).toLowerCase()}`,
        dot: { OPEN: '#3FB950', CLOSED: '#F85149', MERGED: '#A371F7' }[state],
      },
    ]);
    const previous = writes;
    await updatePullRequestTag(service, state);
    assert.equal(writes, previous);
  }
  await assert.rejects(updatePullRequestTag(service, 'unexpected'), /Unknown PR state/);
  assert.equal(tags.length, 2);
  await updatePullRequestTag(service, null);
  assert.deepEqual(tags, [sibling]);
});
