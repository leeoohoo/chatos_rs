import assert from 'node:assert/strict';
import test from 'node:test';
import { UI_LIBRARIES } from '../dist/ui-libraries.test.mjs';

test('component catalogs do not manufacture a uniform three-variant count', () => {
  for (const library of UI_LIBRARIES) {
    assert.equal(library.components.every((component) => (library.variants[component.id] ?? []).length >= 1), true, `${library.id} has a component without an example`);
  }

  for (const libraryId of ['magicui', 'spell', 'inspira', 'daisyui']) {
    const library = UI_LIBRARIES.find((candidate) => candidate.id === libraryId);
    assert.ok(library);
    assert.equal(library.components.every((component) => library.variants[component.id].length === 3), false, `${libraryId} still forces every component to three examples`);
  }

  const magic = UI_LIBRARIES.find((library) => library.id === 'magicui');
  assert.ok(magic);
  assert.equal(magic.variants.Confetti.length, 8);
  assert.equal(magic.variants.TextAnimate.length, 9);
  assert.equal(magic.variants.Marquee.length, 5);
  assert.equal(new Set(magic.variants.Confetti.map((variant) => variant.props.registryDemo)).size, 8);
});

test('Chakra OverlayManager examples represent different content structures', () => {
  const chakra = UI_LIBRARIES.find((library) => library.id === 'chakra');
  assert.ok(chakra);
  const variants = chakra.variants.OverlayManager;
  assert.deepEqual(variants.map((variant) => variant.id), [
    'chakra-overlay-basic',
    'chakra-overlay-with-alert',
    'chakra-overlay-with-drawer',
    'chakra-overlay-with-form',
    'chakra-overlay-with-menu-item',
    'chakra-overlay-with-return-value',
    'chakra-overlay-with-update'
  ]);
  assert.deepEqual(variants.map((variant) => variant.props.registryDemo), variants.map((variant) => variant.id));
  assert.equal(new Set(variants.map((variant) => variant.label)).size, variants.length);
});
