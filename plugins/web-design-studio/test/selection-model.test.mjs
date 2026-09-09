import test from 'node:test';
import assert from 'node:assert/strict';
import {
  deepestSelectionChild,
  normalizedSelectionRect,
  selectionBounds,
  selectionCandidatesAtPoint,
  selectionNodesInRect,
  selectionParent
} from '../dist/selection-model.test.mjs';

const nodes = [
  { id: 'background', name: 'Background', type: 'frame', zIndex: 0, visible: true, rect: { x: 0, y: 0, width: 800, height: 600 } },
  { id: 'card', name: 'Card', type: 'frame', zIndex: 2, visible: true, rect: { x: 100, y: 100, width: 360, height: 240 } },
  { id: 'title', name: 'Title', type: 'text', parentId: 'card', zIndex: 2, visible: true, rect: { x: 130, y: 130, width: 220, height: 48 } },
  { id: 'locked-badge', name: 'Badge', type: 'library-instance', parentId: 'card', zIndex: 4, locked: true, visible: true, rect: { x: 130, y: 130, width: 96, height: 32 } },
  { id: 'hidden', name: 'Hidden', type: 'shape', zIndex: 99, visible: false, rect: { x: 120, y: 120, width: 200, height: 200 } }
];

test('overlap candidates are ordered by visual stacking and exclude hidden nodes', () => {
  assert.deepEqual(selectionCandidatesAtPoint(nodes, { x: 150, y: 150 }).map((item) => item.id), [
    'locked-badge',
    'title',
    'card',
    'background'
  ]);
});

test('nested nodes win ties over their ancestors and preserve selection metadata', () => {
  const candidates = selectionCandidatesAtPoint(nodes, { x: 150, y: 150 });
  assert.equal(candidates[0].locked, true);
  assert.equal(candidates.find((item) => item.id === 'title')?.depth, 1);
  assert.equal(candidates.find((item) => item.id === 'card')?.depth, 0);
});

test('selection navigation enters the topmost direct child and returns to its parent', () => {
  assert.equal(deepestSelectionChild(nodes, 'card')?.id, 'locked-badge');
  assert.equal(selectionParent(nodes, 'title')?.id, 'card');
  assert.equal(selectionParent(nodes, 'background'), undefined);
});

test('point outside all nodes returns no candidates', () => {
  assert.deepEqual(selectionCandidatesAtPoint(nodes, { x: 900, y: 700 }), []);
});

test('marquee rectangles normalize regardless of drag direction', () => {
  assert.deepEqual(normalizedSelectionRect({ x: 420, y: 280 }, { x: 100, y: 80 }), {
    x: 100,
    y: 80,
    width: 320,
    height: 200
  });
});

test('marquee selection prefers fully enclosed nodes and does not accidentally select an enclosing background', () => {
  assert.deepEqual(selectionNodesInRect(nodes, { x: 120, y: 120, width: 250, height: 80 }).map((node) => node.id), [
    'title',
    'locked-badge'
  ]);
});

test('marquee selection falls back to partially intersected nodes and keeps only selection roots', () => {
  assert.deepEqual(selectionNodesInRect(nodes, { x: 440, y: 320, width: 50, height: 50 }).map((node) => node.id), ['card']);
  assert.deepEqual(selectionNodesInRect(nodes, { x: 90, y: 90, width: 400, height: 280 }).map((node) => node.id), ['card']);
});

test('selection bounds merge multiple outlines without changing the source rectangles', () => {
  assert.deepEqual(selectionBounds([
    { x: 10, y: 20, width: 40, height: 30 },
    { x: 80, y: 5, width: 20, height: 70 }
  ]), { x: 10, y: 5, width: 90, height: 70 });
  assert.equal(selectionBounds([]), undefined);
});
