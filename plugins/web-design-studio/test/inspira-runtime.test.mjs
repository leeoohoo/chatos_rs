import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { INSPIRA_BACKGROUND_SLUGS, INSPIRA_COMPONENT_SLUGS, INSPIRA_COMPONENT_VARIANTS } from '../dist/inspira-library.test.mjs';
import { INSPIRA_REGISTRY_ENTRIES } from '../dist/inspira-registry.test.mjs';
import { propsForVueComponent, renderPathForVueRegistryEntry } from '../dist/vue-registry-adapter.test.mjs';

function componentId(slug) {
  return slug.split('-').map((part) => /^3d$/i.test(part) ? 'ThreeD' : `${part[0].toUpperCase()}${part.slice(1)}`).join('');
}

test('all Inspira components are sourced from the official registry through one runtime manifest', () => {
  assert.equal(INSPIRA_REGISTRY_ENTRIES.length, 155);
  assert.equal(INSPIRA_REGISTRY_ENTRIES.flatMap((entry) => entry.demos).length, 197);
  assert.deepEqual(
    [...INSPIRA_REGISTRY_ENTRIES.map((entry) => entry.slug)].sort(),
    [...INSPIRA_COMPONENT_SLUGS].sort()
  );

  const backgrounds = INSPIRA_REGISTRY_ENTRIES.filter((entry) => entry.section === 'backgrounds');
  assert.equal(backgrounds.length, 27);
  assert.deepEqual(
    [...backgrounds.map((entry) => entry.slug)].sort(),
    [...INSPIRA_BACKGROUND_SLUGS].sort()
  );
  assert.equal(new Set(backgrounds.map((entry) => entry.registryId)).size, 27);
  for (const entry of INSPIRA_REGISTRY_ENTRIES) {
    assert.match(entry.docsUrl, new RegExp(`^https://inspira-ui\\.com/docs/en/components/${entry.section}/`));
    assert.ok(entry.componentPaths.includes(entry.rootPath), `${entry.slug} is missing its root component`);
    assert.ok(entry.previewPath, `${entry.slug} is missing its official preview`);
    assert.ok(entry.demos.length >= 1, `${entry.slug} is missing its official examples`);
    for (const sourcePath of new Set([...entry.componentPaths, entry.rootPath, entry.previewPath, ...entry.demos.map((demo) => demo.path)])) {
      assert.ok(existsSync(path.resolve('ui-src/library-runtime', sourcePath.replace(/^\.\//, ''))), `${entry.slug} is missing ${sourcePath}`);
    }
  }
});

test('Inspira exposes every official demo instead of a fixed handcrafted quota', () => {
  assert.equal(INSPIRA_COMPONENT_VARIANTS.PatternBackground.length, 5);
  assert.equal(INSPIRA_COMPONENT_VARIANTS.Ripple.length, 4);
  assert.equal(INSPIRA_COMPONENT_VARIANTS.ThreeDCard.length, 2);
  assert.equal(INSPIRA_COMPONENT_VARIANTS.FlipCard.length, 2);
  assert.equal(INSPIRA_COMPONENT_VARIANTS.GlareCard.length, 2);
  assert.equal(INSPIRA_COMPONENT_VARIANTS.Confetti.length, 5);
  assert.equal(INSPIRA_COMPONENT_VARIANTS.GradientButton.length, 1);
  for (const entry of INSPIRA_REGISTRY_ENTRIES) {
    const variants = INSPIRA_COMPONENT_VARIANTS[componentId(entry.slug)];
    assert.equal(variants.length, entry.demos.length, `${entry.slug} lost official examples`);
    assert.deepEqual(variants.map((variant) => variant.id), entry.demos.map((demo) => demo.id));
    assert.deepEqual(variants.map((variant) => variant.props.registryDemo), entry.demos.map((demo) => demo.id));
  }
});

test('the shared Vue adapter selects official demos through the common registryDemo protocol', () => {
  for (const entry of INSPIRA_REGISTRY_ENTRIES) {
    assert.equal(renderPathForVueRegistryEntry(entry), entry.previewPath, `${entry.slug} default demo is routed incorrectly`);
    for (const demo of entry.demos) {
      assert.equal(renderPathForVueRegistryEntry(entry, { registryDemo: demo.id }), demo.path, `${demo.id} is routed incorrectly`);
    }
  }
  assert.deepEqual(
    propsForVueComponent({ props: { speed: {}, color: {} } }, { speed: 2, color: 'blue', family: 'globe', title: 'ignored' }),
    { speed: 2, color: 'blue' }
  );
  assert.deepEqual(propsForVueComponent({}, { family: 'globe', title: 'ignored' }), {});
});

test('Inspira registry source is preserved without compatibility patches', () => {
  const source = readFileSync('ui-src/library-runtime/vendor/inspira/ui/github-globe/GithubGlobe.vue', 'utf8');
  assert.match(source, /Math\.round\(Math\.random\(\) \* 4\)/);
  assert.match(source, /\.pointsData\(props\.data\)/);
  assert.match(source, /\.pointColor\(\(e: any\) => e\.color\)/);
  assert.doesNotMatch(readFileSync('scripts/sync-inspira-registry.mjs', 'utf8'), /applyInspiraCompatibilityPatches/);
});
