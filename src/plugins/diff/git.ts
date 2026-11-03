import type { ProcessRequest, ProcessResult } from '../../sdk';
import { settings, type Settings } from './settings.ts';
export type CommitOption = { hash: string; subject: string };
export type FileChange = {
  status: string;
  oldPath: string;
  path: string;
  key: string;
  untracked?: boolean;
};
export type Comparison = {
  root: string;
  base: string;
  target: string | null;
  config: Settings;
  files: FileChange[];
  time: number;
  baseLabel: string;
  targetLabel: string;
  detectedBase: string;
};
export type Runner = (request: ProcessRequest) => Promise<ProcessResult>;
export function decodePath(value: string): string {
  if (!value.startsWith('"')) return value;
  if (!value.endsWith('"')) throw new Error('Malformed Git filename.');
  const bytes: number[] = [];
  const escapes: Record<string, number> = {
    a: 7,
    b: 8,
    t: 9,
    n: 10,
    v: 11,
    f: 12,
    r: 13,
    '"': 34,
    '\\': 92,
  };
  for (let i = 1; i < value.length - 1; i++) {
    if (value[i] !== '\\') bytes.push(...new TextEncoder().encode(value[i]));
    else {
      const match = value.slice(i + 1).match(/^[0-7]{3}/);
      if (match) {
        bytes.push(parseInt(match[0], 8));
        i += 3;
      } else {
        const byte = escapes[value[++i]];
        if (byte === undefined) throw new Error('Malformed Git filename escape.');
        bytes.push(byte);
      }
    }
  }
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(new Uint8Array(bytes));
  } catch {
    throw new Error('Non-UTF-8 filenames are unsupported.');
  }
}
export function parseFiles(output: string): FileChange[] {
  return output
    .split('\n')
    .filter(Boolean)
    .map((line) => {
      const [status, first, second] = line.split('\t');
      if (!first || !/^[AMDRTCU][0-9]*$/.test(status))
        throw new Error('Unsupported Git file-list response.');
      const oldPath = decodePath(first),
        path = second === undefined ? oldPath : decodePath(second);
      return { status, oldPath, path, key: JSON.stringify([oldPath, path]) };
    });
}
export class GitComparison {
  private run: Runner;
  constructor(run: Runner) {
    this.run = run;
  }
  async git(cwd: string, args: string[], diffExit = false) {
    const result = await this.run({
      executable: 'git',
      args: [
        '--no-pager',
        '-c',
        'core.quotePath=true',
        '-c',
        'diff.autoRefreshIndex=false',
        ...args,
      ],
      cwd,
      env: { GIT_OPTIONAL_LOCKS: '0', GIT_TERMINAL_PROMPT: '0', GIT_NO_LAZY_FETCH: '1' },
      timeoutMs: 20000,
      outputLimit: 524288,
    });
    if (result.code !== 0 && !(diffExit && result.code === 1))
      throw new Error(
        result.stderr.trim() ||
          `Git exited ${result.code}. Check Git installation, folder and revisions.`,
      );
    return result.stdout;
  }
  async compare(value: Settings): Promise<Comparison> {
    const config = settings(value);
    if (!config.folder) throw new Error('Choose an absolute repository folder.');
    const bare =
      (await this.git(config.folder, ['rev-parse', '--is-bare-repository'])).trim() === 'true';
    if (bare && config.mode === 'branch' && config.includeUncommitted)
      throw new Error('Working directory comparisons require a non-bare repository.');
    // rev-parse emits the root verbatim; remove only its terminating newline.
    const root = bare
      ? config.folder
      : (await this.git(config.folder, ['rev-parse', '--show-toplevel'])).replace(/\n$/, '');
    const resolve = async (ref: string, kind: string) =>
      (
        await this.git(root, ['rev-parse', '--verify', '--end-of-options', `${ref}^{${kind}}`])
      ).trim();
    let baseLabel = config.baseRef;
    const targetLabel = config.mode === 'branch' ? 'HEAD' : config.targetRef;
    let detectedBase = '';
    let base: string;
    if (config.mode === 'branch' && config.compareFrom === 'divergence') {
      let remoteDefault = '';
      try {
        remoteDefault = (
          await this.git(root, ['symbolic-ref', '--quiet', 'refs/remotes/origin/HEAD'])
        ).trim();
      } catch {
        /* No fetch or metadata writes during detection. */
      }
      for (const candidate of [...new Set([remoteDefault, 'main', 'master'].filter(Boolean))]) {
        try {
          await resolve(candidate, 'commit');
          detectedBase = candidate;
          break;
        } catch {
          /* Try the next conventional base. */
        }
      }
      if (!detectedBase)
        throw new Error(
          'Could not detect a base branch. Select a starting commit or compare two refs.',
        );
      const bases = (await this.git(root, ['merge-base', '--all', detectedBase, 'HEAD']))
        .trim()
        .split('\n');
      if (bases.length !== 1 || !bases[0])
        throw new Error('No unique merge base; histories may be unrelated or ambiguous.');
      base = bases[0];
      baseLabel = `Branch divergence (Base: ${detectedBase})`;
    } else {
      baseLabel = config.mode === 'branch' ? config.compareFrom : config.baseRef;
      base = await resolve(baseLabel, 'commit');
    }
    const target =
      config.mode === 'branch' && config.includeUncommitted
        ? null
        : await resolve(targetLabel, 'commit');
    const comparison = {
      root,
      base,
      target,
      config,
      baseLabel,
      detectedBase,
      targetLabel,
      files: [] as FileChange[],
      time: Date.now(),
    };
    comparison.files = parseFiles(
      await this.git(root, [...this.diffArgs(comparison), '--name-status']),
    );
    if (!target && comparison.files.some((file) => file.status === 'M')) {
      // Disabling autoRefreshIndex also disables Git's stat-only filtering.
      // Numstat compares contents without writing the index or returning patches.
      const stats = await this.git(root, [
        ...this.diffArgs(comparison),
        '--no-renames',
        '--numstat',
      ]);
      const changed = new Set(
        stats
          .split('\n')
          .filter(Boolean)
          .map((line) => decodePath(line.split('\t').slice(2).join('\t'))),
      );
      comparison.files = comparison.files.filter(
        (file) => file.status !== 'M' || changed.has(file.path),
      );
    }
    if (!target) {
      const untracked = await this.git(root, ['ls-files', '--others', '--exclude-standard']);
      const existing = new Set(comparison.files.map((file) => file.path));
      for (const line of untracked.split('\n').filter(Boolean)) {
        const path = decodePath(line);
        if (!existing.has(path))
          comparison.files.push({
            status: 'A',
            oldPath: path,
            path,
            key: JSON.stringify([path, path]),
            untracked: true,
          });
      }
      comparison.files.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
    }
    return comparison;
  }
  async commits(folder: string): Promise<CommitOption[]> {
    const output = await this.git(folder, [
      'log',
      '--max-count=200',
      '--format=%H%x00%s',
      'HEAD',
      '--',
    ]);
    return output
      .trimEnd()
      .split('\n')
      .filter(Boolean)
      .map((line) => {
        const separator = line.indexOf('\0');
        return { hash: line.slice(0, separator), subject: line.slice(separator + 1) };
      });
  }
  async references(folder: string): Promise<string[]> {
    return [
      'HEAD',
      'HEAD~1',
      ...(
        await this.git(folder, [
          'for-each-ref',
          '--count=1000',
          '--format=%(refname:short)',
          'refs/heads',
          'refs/remotes',
          'refs/tags',
        ])
      )
        .trim()
        .split('\n')
        .filter(Boolean),
    ];
  }
  diffArgs(c: Comparison) {
    return [
      'diff',
      '--no-relative',
      '--no-ext-diff',
      '--no-textconv',
      '--no-color',
      '--ignore-submodules=none',
      '--submodule=short',
      '--find-renames',
      '--diff-algorithm=myers',
      '--no-indent-heuristic',
      c.base,
      ...(c.target ? [c.target] : []),
    ];
  }
  patch(c: Comparison, file: FileChange, context = 3) {
    if (file.untracked)
      return this.git(
        c.root,
        [
          'diff',
          '--no-index',
          '--no-ext-diff',
          '--no-textconv',
          '--no-color',
          `--unified=${context}`,
          '--src-prefix=a/',
          '--dst-prefix=b/',
          '--',
          '/dev/null',
          file.path,
        ],
        true,
      );
    return this.git(c.root, [
      '--literal-pathspecs',
      ...this.diffArgs(c),
      '--patch',
      `--unified=${context}`,
      '--src-prefix=a/',
      '--dst-prefix=b/',
      '--',
      ...new Set([file.oldPath, file.path]),
    ]);
  }
}
// All operations share a serial lane. Cancellation joins before replacement starts.
export class Lane {
  generation = 0;
  private tail: Promise<unknown> = Promise.resolve();
  private cancel: () => Promise<void>;
  constructor(cancel: () => Promise<void>) {
    this.cancel = cancel;
  }
  replace<T>(work: (current: () => boolean) => Promise<T>): Promise<T | undefined> {
    const generation = ++this.generation;
    const cancelled = this.cancel();
    const next = this.tail
      .catch(() => {})
      .then(async () => {
        await cancelled;
        if (generation !== this.generation) return;
        return work(() => generation === this.generation);
      });
    this.tail = next;
    return next;
  }
  stop() {
    return this.replace(async () => {});
  }
}
