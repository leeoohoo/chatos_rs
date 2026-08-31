import {cp, mkdir, readFile, rm, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = path.join(root, 'dist');
const manifest = JSON.parse(await readFile(path.join(root, 'manifest.json'), 'utf8'));
const packageJson = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8'));

if (manifest.version !== packageJson.version) {
  throw new Error('Extension manifest and package versions must match');
}

await rm(output, {recursive: true, force: true});
await mkdir(output, {recursive: true});
await Promise.all([
  cp(path.join(root, 'src'), path.join(output, 'src'), {recursive: true}),
  cp(path.join(root, 'popup'), path.join(output, 'popup'), {recursive: true}),
  cp(path.join(root, 'README.md'), path.join(output, 'README.md')),
  cp(path.join(root, '..', 'LICENSE'), path.join(output, 'LICENSE')),
  writeFile(path.join(output, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`)
]);

process.stdout.write(`Built unpacked extension at ${output}\n`);
