import type { SceneRect } from './scene-schema.js';
import type { SolvedSceneLayout } from './layout-engine.js';

export interface RenderedSceneNodeMeasurement {
  rect: SceneRect;
  scrollWidth?: number;
  scrollHeight?: number;
}

export type RenderedSceneMeasurements = Record<string, RenderedSceneNodeMeasurement>;

export type SceneLayoutCalibrationIssue =
  | { code: 'missing-rendered-node'; nodeId: string; severity: 'error' }
  | { code: 'unexpected-rendered-node'; nodeId: string; severity: 'warning' }
  | {
      code: 'geometry-mismatch';
      nodeId: string;
      severity: 'error';
      expected: SceneRect;
      actual: SceneRect;
      delta: SceneRect;
      maximumDelta: number;
    }
  | {
      code: 'render-overflow';
      nodeId: string;
      severity: 'error';
      overflowX: number;
      overflowY: number;
    };

export interface SceneLayoutCalibrationReport {
  rootNodeId: string;
  viewportWidth: number;
  tolerance: number;
  passed: boolean;
  comparedNodeCount: number;
  issues: SceneLayoutCalibrationIssue[];
}

function assertRect(rect: SceneRect, nodeId: string): void {
  for (const [property, value] of Object.entries(rect)) {
    if (!Number.isFinite(value) || (property === 'width' || property === 'height') && value < 0) {
      throw new Error(`Rendered measurement ${nodeId}.${property} is invalid.`);
    }
  }
}

export function calibrateSceneLayout(
  solved: SolvedSceneLayout,
  rendered: RenderedSceneMeasurements,
  tolerance = 1
): SceneLayoutCalibrationReport {
  if (!Number.isFinite(tolerance) || tolerance < 0) throw new Error('Scene layout calibration tolerance is invalid.');
  if (!rendered || typeof rendered !== 'object' || Array.isArray(rendered)) throw new Error('Rendered scene measurements are invalid.');
  const issues: SceneLayoutCalibrationIssue[] = [];
  let comparedNodeCount = 0;
  for (const [nodeId, expectedBox] of solved.boxes) {
    const measurement = rendered[nodeId];
    if (!measurement) {
      issues.push({ code: 'missing-rendered-node', nodeId, severity: 'error' });
      continue;
    }
    assertRect(measurement.rect, nodeId);
    comparedNodeCount += 1;
    const expected = { x: expectedBox.x, y: expectedBox.y, width: expectedBox.width, height: expectedBox.height };
    const actual = structuredClone(measurement.rect);
    const delta = {
      x: actual.x - expected.x,
      y: actual.y - expected.y,
      width: actual.width - expected.width,
      height: actual.height - expected.height
    };
    const maximumDelta = Math.max(...Object.values(delta).map(Math.abs));
    if (maximumDelta > tolerance) issues.push({
      code: 'geometry-mismatch', nodeId, severity: 'error', expected, actual, delta, maximumDelta
    });
    const scrollWidth = measurement.scrollWidth ?? actual.width;
    const scrollHeight = measurement.scrollHeight ?? actual.height;
    if (!Number.isFinite(scrollWidth) || scrollWidth < 0 || !Number.isFinite(scrollHeight) || scrollHeight < 0) {
      throw new Error(`Rendered scroll measurement for ${nodeId} is invalid.`);
    }
    const overflowX = Math.max(0, scrollWidth - actual.width);
    const overflowY = Math.max(0, scrollHeight - actual.height);
    if (overflowX > tolerance || overflowY > tolerance) issues.push({
      code: 'render-overflow', nodeId, severity: 'error', overflowX, overflowY
    });
  }
  for (const nodeId of Object.keys(rendered)) {
    if (!solved.boxes.has(nodeId)) {
      assertRect(rendered[nodeId].rect, nodeId);
      issues.push({ code: 'unexpected-rendered-node', nodeId, severity: 'warning' });
    }
  }
  return {
    rootNodeId: solved.rootNodeId,
    viewportWidth: solved.viewportWidth,
    tolerance,
    passed: issues.every((issue) => issue.severity !== 'error'),
    comparedNodeCount,
    issues
  };
}
