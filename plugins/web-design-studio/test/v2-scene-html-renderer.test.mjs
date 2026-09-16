import assert from 'node:assert/strict';
import test from 'node:test';
import { createPhase2LayoutBenchmarks } from '../dist/v2-phase2-layout-benchmarks.test.mjs';
import { renderPhase2BenchmarkScene } from '../dist/v2-scene-html-renderer.test.mjs';
import { createSceneNodeBase } from '../dist/v2-scene-schema.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

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

test('HTML renderer uses unitless line heights and embeds official component runtimes', () => {
  const document = nestedWebsite();
  const heading = document.pages[0].children[0].children[0].children[0].children[0];
  heading.appearance.typography.lineHeight = 1.125;
  const button = {
    ...createSceneNodeBase('library-instance', 'Primary action', { x: 80, y: 300, width: 220, height: 64 }, 'ai'),
    id: 'library-primary-action',
    type: 'library-instance',
    library: 'shadcn',
    component: 'Button',
    content: '开始设计',
    properties: { componentSlug: 'button' },
    slots: {}
  };
  button.layout.position = 'absolute';
  document.pages[0].children[0].children.push(button);

  const rendered = renderPhase2BenchmarkScene({
    benchmarkId: 'runtime-regression',
    name: 'Runtime regression',
    document,
    rootNodeId: 'section-responsive',
    viewportWidths: [800],
    assertions: []
  }, 800);

  assert.match(rendered.html, /line-height:1.125/);
  assert.doesNotMatch(rendered.html, /line-height:1.125px/);
  assert.match(rendered.html, /data-library-runtime-instance="library-primary-action"/);
  assert.match(rendered.html, /library=shadcn/);
  assert.match(rendered.html, /component=button/);
  assert.match(rendered.html, /data-scene-ready="false"/);
});
