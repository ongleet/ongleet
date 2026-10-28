import type { ProcessRequest, ProcessResult } from '../../sdk';
import { settings, type Settings } from './settings.ts';
export type FileChange = { status: string; oldPath: string; path: string; key: string };
export type Comparison = {
  root: string;
  base: string;
  target: string | null;
  config: Settings;
  files: FileChange[];
  time: number;
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
  async git(cwd: string, args: string[]) {
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
    if (result.code !== 0)
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
    if (bare && config.mode === 'workingDirectory')
      throw new Error('Working directory comparisons require a non-bare repository.');
    // rev-parse emits the root verbatim; remove only its terminating newline.
    const root = bare
      ? config.folder
      : (await this.git(config.folder, ['rev-parse', '--show-toplevel'])).replace(/\n$/, '');
    const resolve = async (ref: string, kind: string) =>
      (
        await this.git(root, ['rev-parse', '--verify', '--end-of-options', `${ref}^{${kind}}`])
      ).trim();
    let base = await resolve(config.baseRef, config.mode === 'mergeBase' ? 'commit' : 'tree');
    const target =
      config.mode === 'workingDirectory'
        ? null
        : await resolve(config.targetRef, config.mode === 'mergeBase' ? 'commit' : 'tree');
    if (config.mode === 'mergeBase') {
      const bases = (await this.git(root, ['merge-base', '--all', base, target!]))
        .trim()
        .split('\n');
      if (bases.length !== 1 || !bases[0])
        throw new Error('No unique merge base; histories may be unrelated or ambiguous.');
      base = bases[0];
    }
    const comparison = { root, base, target, config, files: [] as FileChange[], time: Date.now() };
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
    return comparison;
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
  patch(c: Comparison, file: FileChange) {
    return this.git(c.root, [
      '--literal-pathspecs',
      ...this.diffArgs(c),
      '--patch',
      '--unified=3',
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
