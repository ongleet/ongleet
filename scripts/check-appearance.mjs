import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
import vm from 'node:vm';
const modules = {};
for (const name of ['theme-presets', 'appearance']) {
  const exports = {};
  vm.runInNewContext(
    ts.transpileModule(readFileSync(`src/${name}.ts`, 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
    }).outputText,
    {
      exports,
      require: (path) => modules[path.replace('./', '')],
      window: {
        get localStorage() {
          throw Error('unavailable');
        },
      },
    },
  );
  modules[name] = exports;
}
const { createAppearanceStore } = modules.appearance;
function fixture(value = null) {
  let stored = value;
  const store = createAppearanceStore(() => ({
    getItem: () => stored,
    setItem: (_, value) => {
      stored = value;
    },
  }));
  return { store, persisted: () => JSON.parse(stored) };
}
assert.equal(fixture().store.getSnapshot().application.id, 'original');
assert.equal(fixture('{broken').store.getSnapshot().application.id, 'original');
assert.equal(
  fixture('{"version":99,"application":"light-plus"}').store.getSnapshot().application.id,
  'original',
);
const restored = fixture('{"version":1,"application":"light-plus","terminal":"monokai"}');
assert.equal(restored.store.getSnapshot().application.mode, 'light');
assert.equal(restored.store.getSnapshot().terminal.id, 'monokai');
const invalid = fixture(
  '{"version":1,"application":"missing","terminal":"missing"}',
).store.getSnapshot();
assert.equal(invalid.application.id, 'original');
assert.equal(invalid.preferences.terminal, null);
const { store, persisted } = fixture();
let changes = 0;
const unsubscribe = store.subscribe(() => changes++);
store.update({ application: 'one-dark' });
assert.equal(store.getSnapshot().terminal.id, 'one-dark');
store.update({ terminal: 'darcula' });
store.update({ application: 'light-plus' });
assert.equal(store.getSnapshot().terminal.id, 'darcula');
store.update({ terminal: null });
assert.equal(store.getSnapshot().terminal.id, 'light-plus');
assert.equal(persisted().application, 'light-plus');
assert.equal(persisted().terminal, null);
assert.equal(Object.keys(persisted()).sort().join(','), 'application,terminal,version');
assert.equal(changes, 4);
unsubscribe();
unsubscribe();
store.update({ application: 'original' });
assert.equal(changes, 4);
const unavailable = createAppearanceStore(() => {
  throw Error('denied');
});
unavailable.update({ application: 'monokai' });
assert.equal(unavailable.getSnapshot().application.id, 'monokai');
assert.match(unavailable.getSnapshot().persistenceNotice, /could not be saved/);
let fail = true;
const retry = createAppearanceStore(() => ({
  getItem: () => null,
  setItem: () => {
    if (fail) throw Error('quota');
  },
}));
retry.update({ application: 'light-plus' });
assert.ok(retry.getSnapshot().persistenceNotice);
fail = false;
retry.update({ terminal: 'one-dark' });
assert.equal(retry.getSnapshot().persistenceNotice, '');
for (const theme of modules['theme-presets'].themePresets) {
  for (const value of Object.values(theme.ui)) assert.match(value, /^#[0-9a-f]{3,8}$/i);
  for (const value of Object.values(theme.terminal)) assert.match(value, /^#[0-9a-f]{6}$/i);
  assert.equal(Object.keys(theme.terminal).length, 22);
}
console.log(
  'Appearance restoration, validation, inheritance, override, persistence failures/retry, listener cleanup and complete palettes passed.',
);
