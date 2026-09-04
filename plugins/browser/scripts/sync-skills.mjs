import { cp, mkdir, readdir, readFile, rm } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const browserRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sourceRoot = path.join(browserRoot, 'skills');
const packageRoot = path.join(browserRoot, 'npm', 'skills');
const manifest = JSON.parse(await readFile(path.join(browserRoot, 'npm', 'chatos.plugin.json'), 'utf8'));
const declared = manifest.skills.map((entry) => entry.replace(/^\.\/skills\//, ''));
const available = (await readdir(sourceRoot, { withFileTypes: true }))
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();

if (JSON.stringify([...declared].sort()) !== JSON.stringify(available)) {
  throw new Error(`Browser Skill manifest does not match the root Skill source. Declared: ${declared.join(', ')}; available: ${available.join(', ')}`);
}

await rm(packageRoot, { recursive: true, force: true });
await mkdir(packageRoot, { recursive: true });
for (const name of available) {
  await cp(path.join(sourceRoot, name), path.join(packageRoot, name), { recursive: true });
}
