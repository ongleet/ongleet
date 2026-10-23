import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import ts from 'typescript';
async function files(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  return (
    await Promise.all(
      entries.map((entry) =>
        entry.isDirectory()
          ? files(path.join(directory, entry.name))
          : [path.join(directory, entry.name)],
      ),
    )
  ).flat();
}
let failed = false;
for (const file of await files('src/plugins')) {
  // The host creates attachments; plugin entrypoints and implementations cannot.
  if (file === 'src/plugins/index.tsx' || !/\.[jt]sx?$/.test(file)) continue;
  const source = ts.createSourceFile(
    file,
    await readFile(file, 'utf8'),
    ts.ScriptTarget.Latest,
    true,
  );
  function visit(node) {
    let specifier;
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier &&
      ts.isStringLiteral(node.moduleSpecifier)
    )
      specifier = node.moduleSpecifier.text;
    if (
      ts.isCallExpression(node) &&
      (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
        node.expression.getText(source) === 'require') &&
      ts.isStringLiteral(node.arguments[0])
    )
      specifier = node.arguments[0].text;
    if (specifier) {
      const resolved = path.normalize(path.join(path.dirname(file), specifier));
      if (
        specifier.startsWith('@tauri-apps/') ||
        (resolved.startsWith('src/sdk/') &&
          !['src/sdk/index', 'src/sdk/index.tsx'].includes(resolved)) ||
        resolved === 'src/sdk/transport'
      ) {
        console.error(`${file}: forbidden native/SDK internal import ${specifier}`);
        failed = true;
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}
if (failed) process.exit(1);
console.log('Plugin native boundary imports passed.');
