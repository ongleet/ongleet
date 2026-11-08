import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

function compile(file) {
  return ts.transpileModule(readFileSync(file, 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
}
const catalog = {};
vm.runInNewContext(compile('src/locales/fr.ts'), { exports: catalog });
function fixture(saved, blocked = false) {
  const values = new Map([['ongleet.locale', saved]]);
  const document = { documentElement: { lang: '' } };
  const exports = {};
  vm.runInNewContext(compile('src/i18n.ts'), {
    exports,
    document,
    localStorage: {
      getItem: (key) => {
        if (blocked) throw Error('Denied');
        return values.get(key);
      },
      setItem: (key, value) => {
        if (blocked) throw Error('Denied');
        values.set(key, value);
      },
    },
    require: (name) => (name === 'react' ? {} : catalog),
  });
  return { ...exports, values, document };
}
const app = fixture('unsupported');
assert.equal(app.getLocale(), 'en');
assert.equal(app.t('Save'), 'Save');
let notifications = 0;
const unsubscribe = app.subscribeLocale(() => notifications++);
app.setLocale('fr');
assert.equal(app.t('Save'), 'Enregistrer');
assert.equal(app.document.documentElement.lang, 'fr');
assert.equal(fixture(app.values.get('ongleet.locale')).getLocale(), 'fr');
assert.equal(app.t('Close {value0}', { value0: '<café>{value1}' }), 'Fermer <café>{value1}');
assert.equal(app.t('Future untranslated message'), 'Future untranslated message');
assert.equal(notifications, 1);
unsubscribe();
app.setLocale('en');
assert.equal(notifications, 1);
const restricted = fixture('fr', true);
assert.equal(restricted.getLocale(), 'en');
restricted.setLocale('fr');
assert.equal(restricted.t('Save'), 'Enregistrer');
assert.match(restricted.localePersistenceNotice(), /pas pu être enregistrée/);

// Catch missing translations and mismatched interpolations as UI copy is added.
function files(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? files(`${dir}/${entry.name}`) : [`${dir}/${entry.name}`],
  );
}
const placeholders = (text) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
for (const file of files('src').filter(
  (file) => /\.tsx?$/.test(file) && !file.includes('/locales/'),
)) {
  const source = ts.createSourceFile(
    file,
    readFileSync(file, 'utf8'),
    ts.ScriptTarget.Latest,
    true,
  );
  function visit(node) {
    if (
      ts.isCallExpression(node) &&
      node.expression.getText(source) === 't' &&
      node.arguments[0] &&
      ts.isStringLiteralLike(node.arguments[0])
    ) {
      const key = node.arguments[0].text;
      assert.ok(Object.hasOwn(catalog.fr, key), `${file}: missing French translation: ${key}`);
      assert.deepEqual(
        placeholders(catalog.fr[key]),
        placeholders(key),
        `${file}: interpolation mismatch: ${key}`,
      );
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}
console.log(
  'i18n preference, storage failures, interpolation, fallback and catalog coverage passed.',
);
