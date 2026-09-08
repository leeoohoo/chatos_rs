import assert from 'node:assert/strict';
import test from 'node:test';
import { createPhase2LayoutBenchmarks } from '../dist/v2-phase2-layout-benchmarks.test.mjs';
import { renderPhase2BenchmarkScene } from '../dist/v2-scene-html-renderer.test.mjs';

test('phase 2 HTML renderer exposes stable node ids and expected geometry for browser calibration', () => {
  const benchmark = createPhase2LayoutBenchmarks()[0];
  const rendered = renderPhase2BenchmarkScene(benchmark, 390);
  assert.equal(rendered.benchmarkId, benchmark.benchmarkId);
  assert.equal(rendered.viewportWidth, 390);
  assert.ok(rendered.height > 390);
  assert.ok(rendered.nodeCount > 20);
  assert.equal(rendered.diagnostics.some((diagnostic) => diagnostic.severity === 'error'), false);
  assert.match(rendered.html, /id="phase2-scene"/);
  assert.match(rendered.html, /data-scene-ready="true"/);
  assert.match(rendered.html, new RegExp(`data-scene-node-id="${benchmark.rootNodeId}"`));
  assert.match(rendered.html, /data-expected-width="390"/);
  assert.match(rendered.html, /class="scene-node scene-text"/);
  assert.match(rendered.html, /font-family:&quot;Inter&quot;/);
  assert.match(rendered.html, /font-size:20px/);
  assert.match(rendered.html, /overflow-wrap:anywhere/);
  assert.match(rendered.html, /white-space:pre/);
});
