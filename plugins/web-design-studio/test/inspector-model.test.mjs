import assert from 'node:assert/strict';
import test from 'node:test';
import { inspectorCapabilities } from '../dist/inspector-model.test.mjs';

test('inspector only exposes controls that are meaningful for the selected node', () => {
  const text = inspectorCapabilities('heading', { library: false, directChildCount: 0, editableSlotCount: 0 });
  assert.equal(text.content, true);
  assert.equal(text.typography, true);
  assert.equal(text.media, false);
  assert.equal(text.layout, false);

  const image = inspectorCapabilities('image', { library: false, directChildCount: 0, editableSlotCount: 0 });
  assert.equal(image.content, true);
  assert.equal(image.typography, false);
  assert.equal(image.media, true);
});

test('official components use their property contract and slot containers own layout', () => {
  const official = inspectorCapabilities('section', { library: true, directChildCount: 4, editableSlotCount: 1 });
  assert.equal(official.content, false);
  assert.equal(official.library, true);
  assert.equal(official.visualStates, true);
  assert.equal(official.typography, true);
  assert.equal(official.layout, false);
});
