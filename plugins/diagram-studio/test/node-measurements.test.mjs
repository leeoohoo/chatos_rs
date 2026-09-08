import assert from 'node:assert/strict';
import test from 'node:test';
import { measuredNode, rememberNodeMeasurements } from '../dist/node-measurements.test.mjs';

test('keeps React Flow measurements available when domain nodes are recreated during a drag', () => {
  const cache = new Map();
  rememberNodeMeasurements(cache, 'diagram-a', [
    { id: 'root', type: 'dimensions', dimensions: { width: 180, height: 64 } },
    { id: 'child', type: 'dimensions', dimensions: { width: 150, height: 52 } }
  ]);

  const movedRoot = { id: 'root', position: { x: 240, y: 180 } };
  const unchangedChild = { id: 'child', position: { x: 520, y: 180 } };

  assert.deepEqual(measuredNode(cache, 'diagram-a', movedRoot).measured, { width: 180, height: 64 });
  assert.deepEqual(measuredNode(cache, 'diagram-a', unchangedChild).measured, { width: 150, height: 52 });
  assert.equal('measured' in movedRoot, false, 'runtime measurements must not pollute persisted diagram nodes');
});

test('isolates measurements by document even when node ids are reused', () => {
  const cache = new Map();
  rememberNodeMeasurements(cache, 'diagram-a', [
    { id: 'shared', type: 'dimensions', dimensions: { width: 120, height: 44 } }
  ]);
  rememberNodeMeasurements(cache, 'diagram-b', [
    { id: 'shared', type: 'dimensions', dimensions: { width: 260, height: 90 } }
  ]);

  assert.deepEqual(measuredNode(cache, 'diagram-a', { id: 'shared' }).measured, { width: 120, height: 44 });
  assert.deepEqual(measuredNode(cache, 'diagram-b', { id: 'shared' }).measured, { width: 260, height: 90 });
});

test('ignores selection and position changes without discarding known measurements', () => {
  const cache = new Map();
  rememberNodeMeasurements(cache, 'diagram-a', [
    { id: 'node-1', type: 'dimensions', dimensions: { width: 160, height: 56 } }
  ]);
  rememberNodeMeasurements(cache, 'diagram-a', [
    { id: 'node-1', type: 'select' },
    { id: 'node-1', type: 'position' }
  ]);

  assert.deepEqual(measuredNode(cache, 'diagram-a', { id: 'node-1' }).measured, { width: 160, height: 56 });
});
