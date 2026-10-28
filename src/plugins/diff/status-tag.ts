import type { SessionTag } from '../../sdk';

type TagService = {
  listTags(): Promise<SessionTag[]>;
  addTag(tag: SessionTag): Promise<SessionTag[]>;
  removeTag(name: string): Promise<SessionTag[]>;
};

export async function updateDiffTag(
  service: TagService,
  tabId: number,
  count: number,
  current: () => boolean,
) {
  const prefix = `Diff #${tabId}: `;
  const desired = {
    name: `${prefix}${count} ${count === 1 ? 'file' : 'files'}`,
    dot: count ? '#D29922' : '#9198A1',
  };
  const existing = await service.listTags();
  for (const tag of existing) {
    if (!current()) return;
    if (tag.name.startsWith(prefix) && tag.name !== desired.name) await service.removeTag(tag.name);
  }
  if (current() && !existing.some((tag) => tag.name === desired.name && tag.dot === desired.dot))
    await service.addTag(desired);
}
