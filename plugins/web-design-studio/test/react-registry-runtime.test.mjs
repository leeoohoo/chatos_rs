import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';

function generatedEntries(file, constant) {
  const source = readFileSync(file, 'utf8');
  const match = source.match(new RegExp(`export const ${constant} = (\\[[\\s\\S]*?\\]) as const satisfies`));
  assert.ok(match, `${constant} was not generated`);
  return JSON.parse(match[1]);
}

function assertOfficialFiles(entries) {
  for (const entry of entries) {
    for (const sourcePath of new Set([entry.rootPath, entry.previewPath, ...(entry.demos ?? []).map((demo) => demo.path)])) {
      const absolute = path.resolve('ui-src/library-runtime', sourcePath.replace(/^\.\//, ''));
      assert.ok(existsSync(absolute), `${entry.slug} is missing ${sourcePath}`);
    }
  }
}

test('Magic UI uses the official public registry and official demos through the shared React adapter', () => {
  const entries = generatedEntries('ui-src/library-runtime/magicui-registry.generated.ts', 'MAGICUI_REGISTRY_ENTRIES');
  assert.equal(entries.length, 68);
  assert.equal(entries.filter((entry) => entry.demos.length > 0).length, 65);
  assert.equal(entries.flatMap((entry) => entry.demos).length, 123);
  assert.equal(new Set(entries.map((entry) => entry.slug)).size, 68);
  assertOfficialFiles(entries);
  const adapter = readFileSync('ui-src/library-runtime/magicui-adapter.ts', 'utf8');
  assert.match(adapter, /createReactRegistryAdapter/);
  assert.doesNotMatch(adapter, /NEON SYSTEM|Visual system|Case study/);
});

test('Spell UI mounts all 33 official registry source files through the same React adapter', () => {
  const entries = generatedEntries('ui-src/library-runtime/spell-registry.generated.ts', 'SPELL_REGISTRY_ENTRIES');
  assert.equal(entries.length, 33);
  assert.equal(new Set(entries.map((entry) => entry.slug)).size, 33);
  assert.equal(entries.every((entry) => entry.rootPath === entry.previewPath), true);
  assertOfficialFiles(entries);
  const adapter = readFileSync('ui-src/library-runtime/spell-adapter.ts', 'utf8');
  assert.match(adapter, /createReactRegistryAdapter/);
  assert.doesNotMatch(adapter, /creative-card|NEON SYSTEM|Visual system|Case study/);
});

test('official runtime routing has no legacy creative renderer fallback', () => {
  const registry = readFileSync('ui-src/library-runtime/registry.ts', 'utf8');
  const canvas = readFileSync('ui-src/studio/LibraryCanvasComponent.tsx', 'utf8');
  assert.match(registry, /magicui: MAGICUI_REGISTRY_BY_SLUG/);
  assert.match(registry, /spell: SPELL_REGISTRY_BY_SLUG/);
  assert.match(canvas, /hasOfficialRuntimeComponent/);
  assert.doesNotMatch(canvas, /CreativeCanvasComponent|legacy|fallback renderer/);
  assert.equal(existsSync('ui-src/studio/CreativeCanvasComponent.tsx'), false);
});
