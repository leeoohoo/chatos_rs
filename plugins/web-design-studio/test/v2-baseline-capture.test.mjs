import assert from 'node:assert/strict';
import test from 'node:test';
import {
  createBaselineCapturePlan,
  validateBaselineRunDefinition
} from '../dist/v2-baseline-capture.test.mjs';
import { V2_WEBSITE_BENCHMARKS } from '../dist/v2-phase0-baseline.test.mjs';

function completeDefinition() {
  return {
    runId: 'v3.0.1-before-scene-graph',
    sourceVersion: '3.0.1-phase0',
    createdAt: '2026-09-07T00:00:00.000Z',
    designs: V2_WEBSITE_BENCHMARKS.map((benchmark) => ({
      benchmarkId: benchmark.id,
      url: `http://127.0.0.1:4188/?baseline=${benchmark.id}`,
      projectId: 'project-baseline',
      documentId: `design-${benchmark.id}`
    }))
  };
}

test('complete baseline runs expand to twelve websites across ten viewports', () => {
  const definition = completeDefinition();
  assert.deepEqual(validateBaselineRunDefinition(definition), []);
  const plan = createBaselineCapturePlan(definition);
  assert.equal(plan.length, 120);
  assert.equal(new Set(plan.map((target) => target.relativeScreenshotPath)).size, 120);
  assert.match(plan[0].relativeScreenshotPath, /^v3\.0\.1-before-scene-graph\/saas-product\//);
});

test('capture planning rejects incomplete, duplicate, unknown, and unsafe inputs', () => {
  const definition = completeDefinition();
  definition.runId = '../escape';
  definition.designs = [
    { benchmarkId: 'saas-product', url: 'file:///tmp/design.html' },
    { benchmarkId: 'saas-product', url: 'http://127.0.0.1:4188/' },
    { benchmarkId: 'unknown', url: 'http://127.0.0.1:4188/' }
  ];
  const errors = validateBaselineRunDefinition(definition);
  assert.ok(errors.some((error) => error.includes('safe file name')));
  assert.ok(errors.some((error) => error.includes('Duplicate design target')));
  assert.ok(errors.some((error) => error.includes('Unknown benchmark id')));
  assert.ok(errors.some((error) => error.includes('HTTP or HTTPS')));
  assert.ok(errors.some((error) => error.includes('Missing design target')));
});

test('partial capture plans are available only when explicitly requested', () => {
  const definition = completeDefinition();
  definition.designs = definition.designs.slice(0, 1);
  assert.throws(() => createBaselineCapturePlan(definition), /Missing design target/);
  assert.equal(createBaselineCapturePlan(definition, false).length, 10);
});

test('capture mode is explicit and rejects unknown browser loading strategies', () => {
  const definition = completeDefinition();
  definition.designs[0].captureMode = 'html';
  assert.deepEqual(validateBaselineRunDefinition(definition), []);
  definition.designs[0].captureMode = 'iframe-proxy';
  assert.ok(validateBaselineRunDefinition(definition).some((error) => error.includes('captureMode must be navigate or html')));
});

test('viewport-aware previews use only safe query parameter names', () => {
  const definition = completeDefinition();
  definition.designs[0].viewportQuery = 'width';
  assert.deepEqual(validateBaselineRunDefinition(definition), []);
  definition.designs[0].viewportQuery = '../width';
  assert.ok(validateBaselineRunDefinition(definition).some((error) => error.includes('viewportQuery must be a safe')));
});
