import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { SHADCN_COMPONENTS, variantsForShadcnComponent } from '../dist/shadcn-library.test.mjs';
import { SHADCN_REGISTRY_ENTRIES } from '../dist/shadcn-registry.test.mjs';

function compositionModules(node, paths = []) {
  if (Array.isArray(node)) for (const child of node) compositionModules(child, paths);
  else if (node && typeof node === 'object') {
    if (typeof node.module === 'string') paths.push(node.module);
    for (const value of Object.values(node)) compositionModules(value, paths);
  }
  return paths;
}

test('shadcn/ui uses one official registry runtime for every current primitive', () => {
  assert.equal(SHADCN_REGISTRY_ENTRIES.length, 61);
  assert.deepEqual(
    [...SHADCN_REGISTRY_ENTRIES.map((entry) => entry.slug)].sort(),
    [...SHADCN_COMPONENTS.map((entry) => entry.librarySlug ?? entry.id.replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase())].sort()
  );
  for (const entry of SHADCN_REGISTRY_ENTRIES) {
    assert.match(entry.docsUrl, /^https:\/\/ui\.shadcn\.com\/docs\/components\//);
    assert.ok(existsSync(path.resolve('ui-src/library-runtime', entry.rootPath.replace(/^\.\//, ''))));
    assert.ok(existsSync(path.resolve('ui-src/library-runtime', entry.previewPath.replace(/^\.\//, ''))));
    for (const demo of entry.demos ?? []) {
      if (demo.path) assert.ok(existsSync(path.resolve('ui-src/library-runtime', demo.path.replace(/^\.\//, ''))), `${entry.slug} is missing ${demo.id}`);
      else {
        assert.equal(demo.type, 'registry:composition');
        assert.ok(demo.composition, `${entry.slug} composition ${demo.id} is missing its declarative tree`);
        for (const modulePath of compositionModules(demo.composition)) {
          assert.ok(existsSync(path.resolve('ui-src/library-runtime', modulePath.replace(/^\.\//, ''))), `${demo.id} references missing ${modulePath}`);
        }
      }
      assert.equal(variantsForShadcnComponent(entry.slug.split('-').map((part) => part === 'otp' ? 'OTP' : `${part[0].toUpperCase()}${part.slice(1)}`).join('')).some((variant) => variant.id === demo.id), true);
    }
  }
});

test('shadcn/ui variants come from upstream examples, blocks, and declarative official primitive compositions', () => {
  const demos = SHADCN_REGISTRY_ENTRIES.flatMap((entry) => entry.demos ?? []);
  assert.equal(demos.filter((demo) => demo.type !== 'registry:composition').length, 300);
  assert.equal(demos.filter((demo) => demo.type === 'registry:composition').length, 14);
  assert.deepEqual(variantsForShadcnComponent('Button').slice(0, 4).map((variant) => variant.id), [
    'button-demo', 'button-as-child', 'button-default', 'button-destructive'
  ]);
  assert.equal(variantsForShadcnComponent('Chart').length, 76);
  assert.equal(variantsForShadcnComponent('Sidebar').length, 16);
  assert.equal(variantsForShadcnComponent('Attachment').length, 3);
  assert.equal(SHADCN_REGISTRY_ENTRIES.find((entry) => entry.slug === 'chart').layout, 'fill');
  assert.equal(SHADCN_REGISTRY_ENTRIES.find((entry) => entry.slug === 'sidebar').previewSpan, 'wide');
  for (const definition of SHADCN_COMPONENTS) {
    for (const variant of variantsForShadcnComponent(definition.id)) {
      assert.equal(variant.props.registryDemo, variant.id);
    }
  }
  assert.doesNotMatch(readFileSync('ui-src/studio/LibraryCanvasComponent.tsx', 'utf8'), /ShadcnCanvasComponent/);
  assert.equal(existsSync('ui-src/studio/ShadcnCanvasComponent.tsx'), false);
  assert.match(readFileSync('ui-src/studio/WebDesignStudioApp.tsx', 'utf8'), /LazyVariantPreview/);
});
