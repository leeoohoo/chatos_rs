import assert from 'node:assert/strict';
import test from 'node:test';
import { centeredCanvasScroll, createInfiniteCanvasGeometry, panCanvasScroll } from '../dist/infinite-canvas-model.test.mjs';

test('canvas workspace keeps the page centered with substantial pan space in every direction', () => {
  const geometry = createInfiniteCanvasGeometry(1200, 2200);
  assert.deepEqual(geometry, { width: 4800, height: 3800, contentX: 1800, contentY: 800 });
  assert.deepEqual(centeredCanvasScroll(geometry, 1000, 800, 1200, 2200), { left: 1900, top: 800 });
});

test('canvas panning follows the pointer without producing negative scroll positions', () => {
  assert.deepEqual(panCanvasScroll(1000, 800, 120, -50), { left: 880, top: 850 });
  assert.deepEqual(panCanvasScroll(10, 20, 100, 100), { left: 0, top: 0 });
  assert.throws(() => createInfiniteCanvasGeometry(Number.NaN, 100), /invalid/);
});
