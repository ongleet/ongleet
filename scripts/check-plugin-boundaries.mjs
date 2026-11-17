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
for (const entry of await readdir('src/plugins', { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  const root = path.join('src/plugins', entry.name);
  const manifest = JSON.parse(await readFile(path.join(root, 'manifest.json'), 'utf8'));
  if (manifest.icon !== 'assets/icon.svg') {
    throw new Error(`${root}: icon is required and must reference assets/icon.svg`);
  }
  const icon = await readFile(path.join(root, manifest.icon), 'utf8');
  if (!icon.includes('<svg') || !icon.includes('viewBox="0 0 24 24"')) {
    throw new Error(`${root}: icon must be a 24×24 SVG`);
  }
}
for (const nativePath of await files('src/plugins')) {
  const file = nativePath.replaceAll('\\', '/');
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
      const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(file), specifier));
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
