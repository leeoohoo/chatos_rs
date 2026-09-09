import assert from 'node:assert/strict';
import test from 'node:test';
import {
  fitWorkspaceRect,
  fitWorkspaceWidth,
  panWorkspaceCamera,
  parseWorkspaceCamera,
  screenToWorkspace,
  unionWorkspaceRects,
  workspaceArtboardRenderTier,
  workspaceRectIntersectsViewport,
  workspaceToScreen,
  workspaceViewportReady,
  workspaceZoomFromWheel,
  zoomWorkspaceCameraAt
} from '../dist/workspace-camera.test.mjs';

test('workspace waits for a measurable viewport instead of crashing during first layout', () => {
  assert.equal(workspaceViewportReady(undefined), false);
  assert.equal(workspaceViewportReady({ width: 0, height: 800 }), false);
  assert.equal(workspaceViewportReady({ width: 1000, height: 0 }), false);
  assert.equal(workspaceViewportReady({ width: 1000, height: 800 }), true);
});

test('selection bounds combine arbitrary component rectangles in workspace coordinates', () => {
  assert.deepEqual(unionWorkspaceRects([
    { x: 480, y: 260, width: 120, height: 44 },
    { x: 360, y: 420, width: 320, height: 180 },
    { x: 720, y: 300, width: 80, height: 80 }
  ]), { x: 360, y: 260, width: 440, height: 340 });
  assert.equal(unionWorkspaceRects([]), undefined);
});

test('workspace camera pans without finite-world clamps', () => {
  const camera = { x: 120, y: -80, zoom: 1 };
  assert.deepEqual(panWorkspaceCamera(camera, { x: -5000, y: 7200 }), { x: -4880, y: 7120, zoom: 1 });
});

test('workspace viewport visibility accounts for camera pan, zoom, and preload margin', () => {
  const camera = { x: -400, y: -200, zoom: 2 };
  const viewport = { width: 1000, height: 700 };
  assert.equal(workspaceRectIntersectsViewport(camera, { x: 250, y: 150, width: 120, height: 80 }, viewport), true);
  assert.equal(workspaceRectIntersectsViewport(camera, { x: 900, y: 900, width: 120, height: 80 }, viewport), false);
  assert.equal(workspaceRectIntersectsViewport(camera, { x: 720, y: 100, width: 40, height: 40 }, viewport, 100), true);
});

test('artboard rendering advances from placement anchor to shell, content, and runtime by viewport distance', () => {
  const camera = { x: 0, y: 0, zoom: 1 };
  const viewport = { width: 1000, height: 800 };
  assert.equal(workspaceArtboardRenderTier(camera, { x: 2300, y: 100, width: 600, height: 600 }, viewport), 'anchor');
  assert.equal(workspaceArtboardRenderTier(camera, { x: 2100, y: 100, width: 600, height: 600 }, viewport), 'shell');
  assert.equal(workspaceArtboardRenderTier(camera, { x: 1650, y: 100, width: 600, height: 600 }, viewport), 'content');
  assert.equal(workspaceArtboardRenderTier(camera, { x: 1400, y: 100, width: 600, height: 600 }, viewport), 'runtime');
  assert.equal(workspaceArtboardRenderTier(camera, { x: 5000, y: 100, width: 600, height: 600 }, viewport, true), 'runtime');
  assert.throws(() => workspaceArtboardRenderTier(camera, { x: 0, y: 0, width: 1, height: 1 }, viewport, false, { shell: 100, content: 200, runtime: 50 }), /render margins/i);
});

test('pointer-anchored zoom preserves the same workspace coordinate', () => {
  const camera = { x: 100, y: 60, zoom: 0.5 };
  const anchor = { x: 420, y: 280 };
  const before = screenToWorkspace(camera, anchor);
  const zoomed = zoomWorkspaceCameraAt(camera, 2, anchor);
  assert.deepEqual(screenToWorkspace(zoomed, anchor), before);
  assert.deepEqual(workspaceToScreen(zoomed, before), anchor);
});

test('trackpad pinch wheel deltas zoom the canvas and clamp at workspace limits', () => {
  assert.ok(workspaceZoomFromWheel(1, -80) > 1);
  assert.ok(workspaceZoomFromWheel(1, 80) < 1);
  assert.equal(workspaceZoomFromWheel(8, -1000), 8);
  assert.equal(workspaceZoomFromWheel(0.1, 1000), 0.1);
});

test('fit commands center an artboard or keep its readable top edge', () => {
  assert.deepEqual(
    fitWorkspaceRect({ x: 200, y: 100, width: 1200, height: 800 }, { width: 1000, height: 700 }, { top: 50, right: 50, bottom: 50, left: 50 }),
    { x: -100, y: -25, zoom: 0.75 }
  );
  assert.deepEqual(
    fitWorkspaceWidth({ x: 0, y: 0, width: 1440, height: 3000 }, { width: 1200, height: 800 }),
    { x: 48, y: 108, zoom: 23 / 30 }
  );
});

test('stored cameras are validated and zoom is clamped to the 10%–800% range', () => {
  assert.deepEqual(parseWorkspaceCamera({ x: 10, y: 20, zoom: 50 }), { x: 10, y: 20, zoom: 8 });
  assert.deepEqual(parseWorkspaceCamera({ x: 10, y: 20, zoom: 0.01 }), { x: 10, y: 20, zoom: 0.1 });
  assert.equal(parseWorkspaceCamera({ x: '10', y: 20, zoom: 1 }), undefined);
});
