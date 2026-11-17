import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

const source = ts.transpileModule(readFileSync('src/tab-titles.ts', 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const values = new Map();
const localStorage = {
  getItem: (key) => values.get(key) ?? null,
  setItem: (key, value) => values.set(key, value),
  removeItem: (key) => values.delete(key),
};
function restart(storage = localStorage) {
  const exports = {};
  vm.runInNewContext(source, { exports, localStorage: storage });
  return exports;
}
const tabs = [
  { id: 7, isOpen: true },
  { id: 8, isOpen: true },
];
let host = restart();
host.saveTabTitles(3, tabs, { 7: 'shell: ~/project', 8: 'README.md' });
host = restart();
assert.equal(host.loadTabTitles(3, tabs)[7], 'shell: ~/project');
assert.equal(host.loadTabTitles(3, tabs)[8], 'README.md');
assert.equal(host.loadTabTitles(4, tabs)[7], undefined);
const closed = [{ id: 7, isOpen: false }, tabs[1]];
host.saveTabTitles(3, closed, host.loadTabTitles(3, tabs));
assert.equal(restart().loadTabTitles(3, tabs)[7], undefined);
assert.equal(restart().loadTabTitles(3, tabs)[8], 'README.md');
host.saveTabTitles(3, tabs, {});
assert.equal(restart().loadTabTitles(3, tabs)[8], undefined);
values.set('ongleet.tab-titles.v1:3', '{broken');
assert.equal(Object.keys(host.loadTabTitles(3, tabs)).length, 0);
values.set('ongleet.tab-titles.v1:3', JSON.stringify({ 7: 42, 8: 'x'.repeat(257) }));
assert.equal(Object.keys(host.loadTabTitles(3, tabs)).length, 0);
const unavailable = restart({
  getItem() {
    throw new Error('Storage denied');
  },
  setItem() {
    throw new Error('Quota exceeded');
  },
  removeItem() {
    throw new Error('Storage denied');
  },
});
assert.equal(Object.keys(unavailable.loadTabTitles(3, tabs)).length, 0);
assert.doesNotThrow(() => unavailable.saveTabTitles(3, tabs, { 7: 'live title' }));
console.log(
  'Tab titles survive restart, isolate sessions, clear closed/reset tabs and tolerate unavailable storage.',
);
