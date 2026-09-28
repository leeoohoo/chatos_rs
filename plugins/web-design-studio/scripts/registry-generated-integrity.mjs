import { createHash } from 'node:crypto';
import { lstat, mkdir, readFile, readdir, readlink, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

export const REGISTRY_GENERATED_GROUPS = Object.freeze({
  antd: [
    'src/antd-registry.generated.ts',
    'ui-src/library-runtime/antd-registry.generated.ts',
    'ui-src/library-runtime/vendor/antd'
  ],
  chakra: [
    'src/chakra-registry.generated.ts',
    'ui-src/library-runtime/chakra-registry.generated.ts',
    'ui-src/library-runtime/vendor/chakra'
  ],
  shadcn: [
    'src/shadcn-registry.generated.ts',
    'ui-src/library-runtime/shadcn-registry.generated.ts',
    'ui-src/library-runtime/shadcn-registry.generated.css',
    'ui-src/library-runtime/vendor/shadcn'
  ],
  magicui: [
    'src/magicui-registry.generated.ts',
    'ui-src/library-runtime/magicui-registry.generated.ts',
    'ui-src/library-runtime/magicui-registry.generated.css',
    'ui-src/library-runtime/vendor/magicui'
  ],
  spell: [
    'src/spell-registry.generated.ts',
    'ui-src/library-runtime/spell-registry.generated.ts',
    'ui-src/library-runtime/spell-registry.generated.css',
    'ui-src/library-runtime/vendor/spell'
  ],
  inspira: [
    'src/inspira-registry.generated.ts',
    'ui-src/library-runtime/inspira-registry.generated.ts',
    'ui-src/library-runtime/vendor/inspira'
  ],
  daisyui: [
    'src/daisyui-registry.generated.ts',
    'ui-src/library-runtime/daisyui-registry.generated.ts',
    'ui-src/public/daisyui'
  ]
});

async function collectEntries(root, relativePath, entries) {
  const absolutePath = path.join(root, relativePath);
  const metadata = await lstat(absolutePath);
  if (metadata.isDirectory()) {
    const children = await readdir(absolutePath);
    for (const child of children.sort()) {
      await collectEntries(root, path.posix.join(relativePath, child), entries);
    }
    return;
  }
  if (metadata.isSymbolicLink()) {
    entries.push({ path: relativePath, kind: 'link', content: Buffer.from(await readlink(absolutePath)) });
    return;
  }
  if (!metadata.isFile()) throw new Error(`Unsupported generated registry entry: ${relativePath}`);
  entries.push({ path: relativePath, kind: 'file', content: await readFile(absolutePath) });
}

async function hashGeneratedPaths(root, outputPaths) {
  const entries = [];
  for (const outputPath of [...outputPaths].sort()) await collectEntries(root, outputPath, entries);
  const hash = createHash('sha256');
  for (const entry of entries.sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0)) {
    hash.update(entry.kind);
    hash.update('\0');
    hash.update(entry.path);
    hash.update('\0');
    hash.update(entry.content);
    hash.update('\0');
  }
  return hash.digest('hex');
}

export async function buildRegistryLock(root, groups = REGISTRY_GENERATED_GROUPS) {
  const registries = {};
  for (const [name, outputs] of Object.entries(groups)) {
    registries[name] = {
      generator: `scripts/sync-${name}-registry.mjs`,
      outputs,
      sha256: await hashGeneratedPaths(root, outputs)
    };
  }
  return {
    version: 1,
    generatedBy: 'scripts/registry-generated-integrity.mjs --write',
    registries
  };
}

export async function writeRegistryLock(root, lockPath, groups = REGISTRY_GENERATED_GROUPS) {
  const lock = await buildRegistryLock(root, groups);
  await mkdir(path.dirname(lockPath), { recursive: true });
  await writeFile(lockPath, `${JSON.stringify(lock, null, 2)}\n`, 'utf8');
}

export async function verifyRegistryLock(root, lockPath, groups = REGISTRY_GENERATED_GROUPS) {
  const expected = JSON.parse(await readFile(lockPath, 'utf8'));
  const current = await buildRegistryLock(root, groups);
  for (const [name, entry] of Object.entries(current.registries)) {
    if (JSON.stringify(expected.registries?.[name]) !== JSON.stringify(entry)) {
      throw new Error(`${name} registry generated output does not match its integrity lock; run npm run sync:${name}.`);
    }
  }
  if (expected.version !== current.version || Object.keys(expected.registries ?? {}).length !== Object.keys(current.registries).length) {
    throw new Error('Registry generated integrity lock structure is stale; regenerate it through a registry sync command.');
  }
}

const invokedUrl = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : undefined;
if (import.meta.url === invokedUrl) {
  const root = path.resolve(import.meta.dirname, '..');
  const lockPath = path.join(root, 'registry-generated-lock.json');
  const mode = process.argv[2] ?? '--check';
  if (mode === '--write') {
    await writeRegistryLock(root, lockPath);
    console.log('Updated registry-generated-lock.json.');
  } else if (mode === '--check') {
    await verifyRegistryLock(root, lockPath);
    console.log('Registry generated outputs match their integrity lock.');
  } else {
    throw new Error(`Unsupported registry integrity mode: ${mode}`);
  }
}
