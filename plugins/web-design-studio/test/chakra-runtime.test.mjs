import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { CHAKRA_COMPONENTS, variantsForChakraComponent } from '../dist/chakra-library.test.mjs';

function generatedEntries() {
  const source = readFileSync('ui-src/library-runtime/chakra-registry.generated.ts', 'utf8');
  const match = source.match(/export const CHAKRA_REGISTRY_ENTRIES = (\[[\s\S]*?\]) as const satisfies/);
  assert.ok(match, 'Chakra UI registry was not generated');
  return JSON.parse(match[1]);
}

test('Chakra UI uses the complete runnable official compositions catalog', () => {
  const entries = generatedEntries();
  assert.equal(entries.length, 113);
  assert.equal(entries.flatMap((entry) => entry.demos).length, 1152);
  assert.equal(new Set(entries.map((entry) => entry.slug)).size, 113);
  assert.equal(entries.some((entry) => entry.title === 'EnvironmentProvider'), false);
  for (const entry of entries) {
    assert.match(entry.docsUrl, /^https:\/\/chakra-ui\.com\/docs\/components\//);
    assert.ok(entry.demos.length > 0, `${entry.slug} has no official demos`);
    for (const demo of entry.demos) {
      assert.ok(existsSync(path.resolve('ui-src/library-runtime', demo.path.replace(/^\.\//, ''))), `${demo.id} is missing its official source`);
    }
  }
});

test('Chakra palette variants come only from official demos without a legacy renderer', () => {
  const entries = generatedEntries();
  const byTitle = Object.fromEntries(entries.map((entry) => [entry.title, entry]));
  for (const componentId of ['Button', 'List', 'Input', 'Select', 'Card', 'Dialog', 'Drawer']) {
    assert.equal(variantsForChakraComponent(componentId).length, byTitle[componentId].demos.length);
  }
  assert.notEqual(variantsForChakraComponent('Button').length, variantsForChakraComponent('List').length);
  assert.equal(CHAKRA_COMPONENTS.length, 113);
  assert.equal(CHAKRA_COMPONENTS.some((component) => component.id === 'EnvironmentProvider'), false);
  assert.match(readFileSync('ui-src/library-runtime/chakra-adapter.tsx', 'utf8'), /createReactRegistryAdapter/);
  assert.doesNotMatch(readFileSync('src/chakra-library.ts', 'utf8'), /variant\('(?:video|primary|outline)'/);
  assert.equal(existsSync('ui-src/studio/ChakraCanvasComponent.tsx'), false);
  assert.doesNotMatch(readFileSync('ui-src/studio/LibraryCanvasComponent.tsx', 'utf8'), /ChakraCanvasComponent/);
});
