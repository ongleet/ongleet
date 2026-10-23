import type { PluginSdk } from '../../sdk';
export type Association = { repository: string; number: number };
export function parseAssociation(target: string, number?: number): Association | null {
  target = target.trim();
  if (!target) return null;
  if (target.startsWith('https://github.com/')) {
    const parts = target.slice('https://github.com/'.length).replace(/\/+$/, '').split('/');
    if (parts.length !== 4 || parts[2] !== 'pull' || !/^\d+$/.test(parts[3]))
      throw new Error('Expected a GitHub pull request URL');
    target = `${parts[0]}/${parts[1]}`;
    number = Number(parts[3]);
  }
  if (
    target.length > 256 ||
    !/^[A-Za-z0-9_][A-Za-z0-9_.-]*\/[A-Za-z0-9_][A-Za-z0-9_.-]*$/.test(target) ||
    !Number.isSafeInteger(number) ||
    number! <= 0
  )
    throw new Error('Expected owner/repository and a positive PR number');
  return { repository: target, number: number! };
}
export async function fetchPullRequest(sdk: PluginSdk, association: Association): Promise<unknown> {
  let result;
  try {
    result = await sdk.process.run({
      executable: 'gh',
      args: [
        'pr',
        'view',
        String(association.number),
        '--repo',
        association.repository,
        '--json',
        'number,title,url,state,reviewDecision,statusCheckRollup,updatedAt',
      ],
      env: { GH_PROMPT_DISABLED: '1', GH_HOST: 'github.com', GH_PAGER: 'cat' },
      timeoutMs: 20_000,
      outputLimit: 512 * 1024,
    });
  } catch (error) {
    throw new Error(
      `Cannot refresh PR; install/authenticate gh with gh auth login. ${String(error)}`,
    );
  }
  if (result.code !== 0)
    throw new Error(`gh failed (${result.code}): ${result.stderr.slice(0, 8192)}`);
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    throw new Error(`Invalid gh response: ${String(error)}`);
  }
}
