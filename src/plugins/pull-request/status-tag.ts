import type { SessionTag } from '../../sdk';

const tags: Record<string, SessionTag> = {
  OPEN: { name: 'PR: Open', dot: '#3FB950' },
  CLOSED: { name: 'PR: Closed', dot: '#F85149' },
  MERGED: { name: 'PR: Merged', dot: '#A371F7' },
};
type TagService = {
  listTags(): Promise<SessionTag[]>;
  addTag(tag: SessionTag): Promise<SessionTag[]>;
  removeTag(name: string): Promise<SessionTag[]>;
};

export async function updatePullRequestTag(
  service: TagService,
  state: string | null,
  isCurrent: () => boolean = () => true,
) {
  const desired = state === null ? undefined : tags[state];
  if (state !== null && !desired) throw new Error(`Unknown PR state: ${state}`);
  const existing = await service.listTags();
  const names = new Set(Object.values(tags).map((tag) => tag.name.toLowerCase()));
  for (const tag of existing) {
    if (!isCurrent()) return;
    if (names.has(tag.name.toLowerCase()) && tag.name !== desired?.name)
      await service.removeTag(tag.name);
  }
  if (
    isCurrent() &&
    desired &&
    !existing.some((tag) => tag.name === desired.name && tag.dot === desired.dot)
  )
    await service.addTag(desired);
}
