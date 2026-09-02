import assert from 'node:assert/strict';
import test from 'node:test';
import { nextNodeZIndex, reorderNodeLayers } from '../dist/layers.test.mjs';

function node(id, parentId, zIndex) {
  return {
    id,
    type: 'diagramNode',
    position: { x: 0, y: 0 },
    parentId,
    zIndex,
    data: { label: id, category: 'process', shape: 'rectangle' }
  };
}

function orderedIds(nodes, parentId) {
  return nodes
    .filter((item) => item.parentId === parentId)
    .sort((left, right) => left.zIndex - right.zIndex)
    .map((item) => item.id);
}

test('layer actions move a selected group while preserving its internal order', () => {
  const nodes = [node('a'), node('b'), node('c'), node('d')];
  assert.deepEqual(orderedIds(reorderNodeLayers(nodes, ['b', 'c'], 'front')), ['a', 'd', 'b', 'c']);
  assert.deepEqual(orderedIds(reorderNodeLayers(nodes, ['b', 'c'], 'back')), ['b', 'c', 'a', 'd']);
  assert.deepEqual(orderedIds(reorderNodeLayers(nodes, ['b'], 'forward')), ['a', 'c', 'b', 'd']);
  assert.deepEqual(orderedIds(reorderNodeLayers(nodes, ['c'], 'backward')), ['a', 'c', 'b', 'd']);
});

test('layer actions stay inside each parent container', () => {
  const nodes = [node('a'), node('b'), node('child-a', 'lane'), node('child-b', 'lane')];
  const reordered = reorderNodeLayers(nodes, ['b', 'child-a'], 'front');
  assert.deepEqual(orderedIds(reordered, undefined), ['a', 'b']);
  assert.deepEqual(orderedIds(reordered, 'lane'), ['child-b', 'child-a']);
  assert.equal(nextNodeZIndex(nodes), 2);
  assert.equal(nextNodeZIndex(nodes, 'lane'), 3);
});
