import assert from 'node:assert/strict';
import test from 'node:test';
import { sameLibraryRuntimeBoundary } from '../dist/render-boundaries.test.mjs';

test('official library runtimes skip unrelated parent renders but update for component, token, mode, or slot changes', () => {
  const component = { id: 'button-one' };
  const tokens = { primary: '#07f' };
  const base = { component, tokens, preview: false, showcase: false, pickItems: false, slotContent: {} };
  assert.equal(sameLibraryRuntimeBoundary(base, { ...base, slotContent: {} }), true);
  assert.equal(sameLibraryRuntimeBoundary(base, { ...base, component: { ...component } }), false);
  assert.equal(sameLibraryRuntimeBoundary(base, { ...base, preview: true }), false);
  assert.equal(sameLibraryRuntimeBoundary(base, { ...base, tokens: { ...tokens } }), false);
  const content = {};
  assert.equal(sameLibraryRuntimeBoundary({ ...base, slotContent: { content } }, { ...base, slotContent: { content } }), true);
  assert.equal(sameLibraryRuntimeBoundary({ ...base, slotContent: { content } }, { ...base, slotContent: { content: {} } }), false);
});
