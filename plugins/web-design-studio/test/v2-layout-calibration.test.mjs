import assert from 'node:assert/strict';
import test from 'node:test';
import { calibrateSceneLayout } from '../dist/v2-layout-calibration.test.mjs';

function solvedLayout() {
  return {
    documentId: 'scene-calibration', revision: 1, rootNodeId: 'frame-root', viewportWidth: 390,
    activeResponsiveRuleIds: [], variableModes: {}, diagnostics: [],
    boxes: new Map([
      ['frame-root', { nodeId: 'frame-root', parentId: 'page-home', pageId: 'page-home', x: 0, y: 0, width: 390, height: 600, contentWidth: 390, contentHeight: 600, overflowX: false, overflowY: false }],
      ['text-heading', { nodeId: 'text-heading', parentId: 'frame-root', pageId: 'page-home', x: 24, y: 40, width: 342, height: 72, contentWidth: 342, contentHeight: 72, overflowX: false, overflowY: false }]
    ])
  };
}

test('browser calibration passes geometry within tolerance', () => {
  const report = calibrateSceneLayout(solvedLayout(), {
    'frame-root': { rect: { x: 0, y: 0, width: 390, height: 600 }, scrollWidth: 390, scrollHeight: 600 },
    'text-heading': { rect: { x: 24.4, y: 39.7, width: 341.5, height: 72.5 } }
  }, 1);
  assert.equal(report.passed, true);
  assert.equal(report.comparedNodeCount, 2);
  assert.deepEqual(report.issues, []);
});

test('browser calibration reports missing, unexpected, shifted, and overflowing nodes separately', () => {
  const report = calibrateSceneLayout(solvedLayout(), {
    'text-heading': { rect: { x: 30, y: 40, width: 340, height: 72 }, scrollWidth: 410, scrollHeight: 90 },
    'runtime-extra': { rect: { x: 0, y: 0, width: 10, height: 10 } }
  }, 1);
  assert.equal(report.passed, false);
  assert.equal(report.comparedNodeCount, 1);
  assert.ok(report.issues.some((issue) => issue.code === 'missing-rendered-node' && issue.nodeId === 'frame-root'));
  assert.ok(report.issues.some((issue) => issue.code === 'unexpected-rendered-node' && issue.nodeId === 'runtime-extra'));
  const geometry = report.issues.find((issue) => issue.code === 'geometry-mismatch');
  assert.equal(geometry.nodeId, 'text-heading');
  assert.equal(geometry.delta.x, 6);
  const overflow = report.issues.find((issue) => issue.code === 'render-overflow');
  assert.equal(overflow.overflowX, 70);
  assert.equal(overflow.overflowY, 18);
});

test('browser calibration rejects malformed measurements instead of hiding them', () => {
  assert.throws(() => calibrateSceneLayout(solvedLayout(), {
    'frame-root': { rect: { x: 0, y: 0, width: Number.NaN, height: 600 } }
  }), /measurement frame-root.width is invalid/);
  assert.throws(() => calibrateSceneLayout(solvedLayout(), {}, -1), /tolerance is invalid/);
});
