import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { ANTD_COMPONENTS, variantsForAntdComponent } from '../dist/antd-library.test.mjs';

function generatedEntries() {
  const source = readFileSync('ui-src/library-runtime/antd-registry.generated.ts', 'utf8');
  const match = source.match(/export const ANTD_REGISTRY_ENTRIES = (\[[\s\S]*?\]) as const satisfies/);
  assert.ok(match, 'Ant Design registry was not generated');
  return JSON.parse(match[1]);
}

test('Ant Design uses independently runnable official documentation demos', () => {
  const entries = generatedEntries();
  assert.equal(entries.length, 72);
  assert.equal(entries.flatMap((entry) => entry.demos).length, 828);
  assert.equal(new Set(entries.map((entry) => entry.slug)).size, 72);
  assert.equal(entries.flatMap((entry) => entry.demos).some((demo) => /debug|semantic|component-token/.test(demo.id)), false);
  for (const entry of entries) {
    assert.match(entry.docsUrl, /^https:\/\/ant\.design\/components\//);
    assert.ok(entry.demos.length > 0, `${entry.slug} has no official demos`);
    for (const demo of entry.demos) {
      assert.ok(existsSync(path.resolve('ui-src/library-runtime', demo.path.replace(/^\.\//, ''))), `${demo.id} is missing its official source`);
    }
  }
});

test('Ant Design palette variants are generated from the official registry instead of a fixed quota', () => {
  const entries = generatedEntries();
  const bySlug = Object.fromEntries(entries.map((entry) => [entry.slug, entry]));
  assert.equal(variantsForAntdComponent('Form').length, bySlug.form.demos.length);
  assert.equal(variantsForAntdComponent('Table').length, bySlug.table.demos.length);
  assert.equal(variantsForAntdComponent('Select').length, bySlug.select.demos.length);
  assert.notEqual(variantsForAntdComponent('Form').length, variantsForAntdComponent('Button').length);
  assert.equal(ANTD_COMPONENTS.length, 72);
  assert.match(readFileSync('ui-src/library-runtime/antd-adapter.ts', 'utf8'), /createReactRegistryAdapter/);
  assert.doesNotMatch(readFileSync('src/antd-library.ts', 'utf8'), /primary-gradient|inline-login|grouped-list/);
  assert.equal(existsSync('ui-src/studio/AntdCanvasComponent.tsx'), false);
  assert.doesNotMatch(readFileSync('ui-src/studio/LibraryCanvasComponent.tsx', 'utf8'), /AntdCanvasComponent/);
});
