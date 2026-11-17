import type { SessionTag } from '../../../sdk';

type TagService = {
  setTag(key: string, tag: SessionTag): Promise<SessionTag[]>;
};

/** Publish this tab's status under a stable key; the host owns close cleanup. */
export async function updateDiffTag(
  service: TagService,
  tabId: number,
  count: number,
  current: () => boolean,
) {
  if (!current()) return;
  await service.setTag('diff-status', {
    name: `Diff #${tabId}: ${count} ${count === 1 ? 'file' : 'files'}`,
    dot: count ? '#D29922' : '#3FB950',
  });
}
