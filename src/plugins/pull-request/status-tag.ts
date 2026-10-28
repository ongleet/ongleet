import type { SessionTag } from '../../sdk';

const tags: Record<string, SessionTag> = {
  DRAFT: { name: 'PR: Draft', dot: '#9198A1' },
  OPEN: { name: 'PR: Open', dot: '#3FB950' },
  CLOSED: { name: 'PR: Closed', dot: '#F85149' },
  MERGED: { name: 'PR: Merged', dot: '#A371F7' },
};
export function pullRequestStatus(pr: { state: string; isDraft: boolean }): string {
  return pr.state === 'OPEN' && pr.isDraft ? 'DRAFT' : pr.state;
}

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

export function pipelineStatus(checks: import('./github').Check[]): string {
  if (!checks.length) return 'No checks';
  const states = checks.map((check) =>
    check.status && check.status !== 'COMPLETED'
      ? 'PENDING'
      : check.conclusion || check.state || 'UNKNOWN',
  );
  if (
    states.some((state) =>
      [
        'FAILURE',
        'ERROR',
        'TIMED_OUT',
        'CANCELLED',
        'ACTION_REQUIRED',
        'STARTUP_FAILURE',
        'STALE',
      ].includes(state),
    )
  )
    return 'Failed';
  if (states.some((state) => state === 'PENDING' || state === 'EXPECTED')) return 'Pending';
  if (states.every((state) => ['SUCCESS', 'NEUTRAL', 'SKIPPED'].includes(state))) return 'Passed';
  return 'Unknown';
}

export async function updatePipelineTag(
  service: TagService,
  failed: boolean,
  isCurrent: () => boolean = () => true,
) {
  const desired = { name: 'PR: Pipeline failed', dot: '#F85149' };
  const existing = (await service.listTags()).find((tag) => tag.name === desired.name);
  if (!isCurrent()) return;
  if (failed && (!existing || existing.dot !== desired.dot)) await service.addTag(desired);
  else if (!failed && existing) await service.removeTag(desired.name);
}
