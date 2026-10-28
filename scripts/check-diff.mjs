import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, writeFile, rm, rename, chmod, symlink, mkdir, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { parseDiff } from 'react-diff-view';
import { GitComparison, decodePath, Lane } from '../src/plugins/diff/git.ts';
const exec = promisify(execFile);
const root = await mkdtemp(join(tmpdir(), 'ongleet-diff-'));
const command = (args) => exec('git', args, { cwd: root });
const runner = async (r) => {
  try {
    const result = await exec(r.executable, r.args, {
      cwd: r.cwd,
      env: { ...process.env, ...r.env },
      maxBuffer: r.outputLimit,
    });
    return { ...result, code: 0 };
  } catch (e) {
    if (e.code === 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER') throw e;
    return { code: e.code, stdout: e.stdout, stderr: e.stderr };
  }
};
const git = new GitComparison(runner);
const config = { folder: root, mode: 'workingDirectory', baseRef: 'HEAD', targetRef: 'HEAD' };
try {
  await command(['init', '-b', 'main']);
  await command(['config', 'user.email', 'fixture@example.test']);
  await command(['config', 'user.name', 'Fixture']);
  const names = ['normal', 'space name', 'café', 'tab\tname', 'line\nname'];
  for (const name of names) await writeFile(join(root, name), 'one\ntwo\nthree\n');
  await writeFile(join(root, '.gitignore'), 'ignored\n');
  await command(['add', '.']);
  await command(['commit', '-m', 'base']);
  await command(['tag', '-a', 'base-tag', '-m', 'base']);
  const initial = await git.compare({ ...config, mode: 'auto' });
  assert.equal(initial.files.length, 6, 'initial commit compares against empty tree');
  const indexBefore = await readFile(join(root, '.git/index'));
  await writeFile(join(root, 'café'), 'one\ntwo\nthree\n');
  await git.compare(config);
  assert.deepEqual(
    await readFile(join(root, '.git/index')),
    indexBefore,
    'stat-only changes must not refresh the index',
  );
  await writeFile(join(root, 'normal'), 'staged\n');
  await command(['add', 'normal']);
  assert.equal((await git.compare(config)).files.length, 1, 'staged-only change');
  await writeFile(join(root, 'normal'), 'one\ntwo\nthree\n');
  assert.equal((await git.compare(config)).files.length, 0, 'staged/unstaged cancellation');
  await writeFile(join(root, 'untracked'), 'hidden');
  await writeFile(join(root, 'ignored'), 'hidden');
  for (const name of names) await writeFile(join(root, name), 'changed\n');
  let c = await git.compare(config);
  assert.equal(c.files.length, names.length);
  for (const file of c.files) {
    const parsed = parseDiff(await git.patch(c, file));
    assert.equal(parsed.length, 1);
    assert.ok(parsed[0].hunks.length);
  }
  assert.deepEqual(new Set(c.files.map((f) => f.path)), new Set(names));
  await rm(join(root, 'untracked'));
  await command(['add', '.']);
  await command(['commit', '-m', 'next']);
  c = await git.compare({ ...config, mode: 'refs', baseRef: 'base-tag', targetRef: 'HEAD' });
  assert.equal(c.files.length, 5);
  const fixed = await git.patch(c, c.files[0]);
  await writeFile(join(root, 'normal'), 'later');
  await command(['add', '.']);
  await command(['commit', '-m', 'move']);
  assert.equal(await git.patch(c, c.files[0]), fixed);
  assert.equal((await git.compare({ ...config, mode: 'refs', baseRef: 'HEAD~1' })).files.length, 1);
  const automaticMain = await git.compare({ ...config, mode: 'auto' });
  assert.deepEqual(
    automaticMain.files.map((f) => f.path),
    ['normal'],
    'default branch shows last commit',
  );
  await command(['checkout', '--detach', 'HEAD']);
  assert.deepEqual(
    (await git.compare({ ...config, mode: 'auto' })).files.map((f) => f.path),
    ['normal'],
    'detached HEAD shows last commit',
  );
  await command(['checkout', 'main']);
  const refs = await git.references(root);
  assert.ok(refs.includes('main') && refs.includes('base-tag'));
  const tree = (await command(['rev-parse', 'HEAD^{tree}'])).stdout.trim();
  const commitTree = async (parents) =>
    (
      await command(['commit-tree', tree, ...parents.flatMap((p) => ['-p', p]), '-m', 'crisscross'])
    ).stdout.trim();
  const ancestor = (await command(['rev-parse', 'HEAD'])).stdout.trim();
  const left = await commitTree([ancestor]);
  // Distinguish commits even when they share the same tree and timestamp.
  const rightUnique = (
    await command(['commit-tree', tree, '-p', ancestor, '-m', 'right'])
  ).stdout.trim();
  const a = await commitTree([left, rightUnique]),
    b = await commitTree([rightUnique, left]);
  await assert.rejects(
    git.compare({ ...config, mode: 'mergeBase', baseRef: a, targetRef: b }),
    /unique merge base/,
  );
  await command(['checkout', '-b', 'side', 'base-tag']);
  await writeFile(join(root, 'side'), 'side');
  await command(['add', '.']);
  await command(['commit', '-m', 'side']);
  const direct = await git.compare({ ...config, mode: 'refs', baseRef: 'main', targetRef: 'side' });
  const merged = await git.compare({
    ...config,
    mode: 'mergeBase',
    baseRef: 'main',
    targetRef: 'side',
  });
  assert.ok(direct.files.length > merged.files.length);
  assert.equal(merged.files[0].path, 'side');
  const automaticSide = await git.compare({ ...config, mode: 'auto' });
  assert.deepEqual(
    automaticSide.files.map((f) => f.path),
    ['side'],
    'feature branch excludes unrelated main changes',
  );
  assert.equal(automaticSide.base, merged.base);
  await rename(join(root, 'normal'), join(root, 'renamed'));
  await command(['add', '.']);
  await chmod(join(root, 'space name'), 0o755);
  await symlink('side', join(root, 'link'));
  await command(['add', 'link']);
  await writeFile(join(root, 'binary'), Buffer.from([0, 1, 2]));
  await command(['add', 'binary']);
  c = await git.compare(config);
  assert.ok(c.files.some((f) => f.status.startsWith('R')));
  assert.ok(
    (
      await git.patch(
        c,
        c.files.find((f) => f.path === 'binary'),
      )
    ).includes('Binary'),
  );
  assert.ok(
    (
      await git.patch(
        c,
        c.files.find((f) => f.path === 'space name'),
      )
    ).includes('mode'),
  );
  assert.ok(
    (
      await git.patch(
        c,
        c.files.find((f) => f.path === 'link'),
      )
    ).includes('120000'),
  );
  await rm(join(root, '.gitignore'));
  await rm(join(root, 'side'));
  await symlink('renamed', join(root, 'side'));
  c = await git.compare(config);
  assert.equal(c.files.find((f) => f.path === '.gitignore').status, 'D');
  assert.equal(c.files.find((f) => f.path === 'side').status, 'T');
  const hash = (await command(['rev-parse', 'HEAD'])).stdout.trim();
  await command(['clone', '.', 'module']);
  await command(['update-index', '--add', '--cacheinfo', `160000,${hash},module`]);
  c = await git.compare(config);
  assert.ok(
    (
      await git.patch(
        c,
        c.files.find((f) => f.path === 'module'),
      )
    ).includes('Subproject commit'),
  );
  await writeFile(join(root, 'large'), 'line\n'.repeat(110000));
  await command(['add', 'large']);
  c = await git.compare(config);
  await assert.rejects(
    git.patch(
      c,
      c.files.find((f) => f.path === 'large'),
    ),
    /maxBuffer/,
  );
  await command(['reset', '--hard', 'HEAD']);
  const invalidPath = Buffer.concat([Buffer.from(root + '/'), Buffer.from([255])]);
  await writeFile(invalidPath, 'unsupported');
  await command(['add', '.']);
  await assert.rejects(git.compare(config), /Non-UTF/);
  await rm(invalidPath);
  await command(['reset', '--hard', 'HEAD']);
  const linked = join(root, 'linked');
  await command(['worktree', 'add', '--detach', linked, 'HEAD']);
  await mkdir(join(linked, 'subdirectory'));
  assert.equal(
    (await git.compare({ ...config, folder: join(linked, 'subdirectory') })).root,
    linked,
  );
  const bare = join(root, 'bare.git');
  await command(['clone', '--bare', '.', bare]);
  await assert.rejects(git.compare({ ...config, folder: bare }), /non-bare/);
  assert.equal(
    (
      await git.compare({
        ...config,
        folder: bare,
        mode: 'refs',
        baseRef: 'HEAD',
        targetRef: 'HEAD',
      })
    ).files.length,
    0,
  );
  const unborn = join(root, 'unborn');
  await mkdir(unborn);
  await command(['init', unborn]);
  await assert.rejects(git.compare({ ...config, folder: unborn }));
  await command(['checkout', '--orphan', 'unrelated']);
  await command(['rm', '-rf', '.']);
  await writeFile(join(root, 'unrelated'), 'new history');
  await command(['add', 'unrelated']);
  await command(['commit', '-m', 'unrelated']);
  await assert.rejects(
    git.compare({ ...config, mode: 'mergeBase', baseRef: 'main', targetRef: 'unrelated' }),
  );
  await assert.rejects(git.compare({ ...config, baseRef: 'missing' }));
  await assert.rejects(git.compare({ ...config, baseRef: '--help' }));
  assert.equal(decodePath('"caf\\303\\251\\tname"'), 'café\tname');
  assert.throws(() => decodePath('"\\377"'), /Non-UTF/);
  const text =
    'diff --git a/a b/a\nindex 1234567..abcdef0 100644\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file\n@@ -10 +10 @@\n-last\n+end\n';
  assert.equal(parseDiff(text)[0].hunks.length, 2);
  await writeFile(
    join(root, 'hunks'),
    Array.from({ length: 30 }, (_, i) => `line ${i}`).join('\n'),
  );
  await command(['add', 'hunks']);
  await command(['commit', '-m', 'hunks']);
  await writeFile(
    join(root, 'hunks'),
    Array.from({ length: 30 }, (_, i) => (i === 0 || i === 29 ? `edit ${i}` : `line ${i}`)).join(
      '\n',
    ),
  );
  c = await git.compare(config);
  const hunks = parseDiff(
    await git.patch(
      c,
      c.files.find((f) => f.path === 'hunks'),
    ),
  )[0].hunks;
  assert.equal(hunks.length, 2);
  await mkdir(join(root, 'many'));
  await Promise.all(
    Array.from({ length: 2800 }, (_, i) =>
      writeFile(join(root, 'many', `${i}-` + 'x'.repeat(200)), 'x'),
    ),
  );
  await command(['add', 'many']);
  await assert.rejects(git.compare(config), /maxBuffer/);
  let release;
  const lane = new Lane(async () => {});
  let applied = 0;
  const first = lane.replace(async (current) => {
    await new Promise((r) => (release = r));
    if (current()) applied++;
  });
  await new Promise((r) => setTimeout(r, 0));
  const second = lane.replace(async (current) => {
    if (current()) applied += 10;
  });
  const third = lane.replace(async (current) => {
    if (current()) applied += 100;
  });
  release();
  await Promise.all([first, second, third]);
  assert.equal(applied, 100);
  console.log(
    'Diff real-Git modes, fixed endpoints, paths, metadata, parser and cancellation passed.',
  );
} finally {
  await rm(root, { recursive: true, force: true });
}
