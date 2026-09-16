import assert from 'node:assert/strict';
import test from 'node:test';
import { scenePointerDelta } from '../dist/scene-pointer-transform.test.mjs';

test('pointer deltas preserve the canvas distance at low zoom', () => {
  assert.deepEqual(
    scenePointerDelta({ clientX: 420, clientY: 160 }, { clientX: 450, clientY: 190 }, 0.3),
    { deltaX: 100, deltaY: 100 }
  );
});

test('pointer deltas always use the fixed gesture origin instead of accumulating preview drift', () => {
  const start = { clientX: 596.5, clientY: 627.5 };
  assert.deepEqual(scenePointerDelta(start, { clientX: 611.5, clientY: 633.5 }, 0.3), { deltaX: 50, deltaY: 20 });
  assert.deepEqual(scenePointerDelta(start, { clientX: 626.5, clientY: 642.5 }, 0.3), { deltaX: 100, deltaY: 50 });
});

test('pointer deltas reject an invalid or collapsed scale', () => {
  assert.throws(() => scenePointerDelta({ clientX: 0, clientY: 0 }, { clientX: 10, clientY: 10 }, 0), /scale must be positive/);
});
