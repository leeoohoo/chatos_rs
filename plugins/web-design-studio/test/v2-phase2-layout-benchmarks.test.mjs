import assert from 'node:assert/strict';
import test from 'node:test';
import { isSceneContainer, isSceneSlotContainer } from '../dist/v2-scene-schema.test.mjs';
import { solveSceneLayout } from '../dist/v2-layout-engine.test.mjs';
import { resolveResponsiveScene } from '../dist/v2-responsive-scene.test.mjs';
import {
  PHASE2_LAYOUT_VIEWPORT_WIDTHS,
  createPhase2LayoutBenchmarks,
  validatePhase2LayoutBenchmarkCatalog
} from '../dist/v2-phase2-layout-benchmarks.test.mjs';

function childrenOf(node) {
  if (isSceneContainer(node)) return node.children;
  if (isSceneSlotContainer(node)) return Object.values(node.slots).flat();
  return [];
}

function walk(nodes, visit) {
  for (const node of nodes) {
    visit(node);
    walk(childrenOf(node), visit);
  }
}

function positiveOverlap(a, b) {
  return Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x) > 0.01
    && Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y) > 0.01;
}

function flowOverlapErrors(document, solved) {
  const errors = [];
  walk(document.pages.flatMap((page) => page.children), (node) => {
    const flowChildren = childrenOf(node).filter((child) => child.visible && child.layout.position === 'flow' && solved.boxes.has(child.id));
    for (let left = 0; left < flowChildren.length; left += 1) {
      for (let right = left + 1; right < flowChildren.length; right += 1) {
        const a = solved.boxes.get(flowChildren[left].id);
        const b = solved.boxes.get(flowChildren[right].id);
        if (positiveOverlap(a, b)) errors.push(`${node.id}: ${a.nodeId} overlaps ${b.nodeId}`);
      }
    }
  });
  return errors;
}

test('phase 2 benchmark catalog contains twelve genuinely different website structures', () => {
  const benchmarks = createPhase2LayoutBenchmarks();
  assert.deepEqual(validatePhase2LayoutBenchmarkCatalog(benchmarks), []);
  assert.equal(benchmarks.length, 12);
  assert.equal(new Set(benchmarks.map((benchmark) => benchmark.pattern)).size, 12);
  assert.equal(new Set(benchmarks.map((benchmark) => benchmark.structuralSignature)).size, 12);
});

test('twelve website structures solve across all ten viewport widths without layout corruption', () => {
  const benchmarks = createPhase2LayoutBenchmarks();
  assert.deepEqual(PHASE2_LAYOUT_VIEWPORT_WIDTHS, [320, 390, 768, 1024, 1280, 1440, 1920, 2560, 3840, 7680]);
  for (const benchmark of benchmarks) {
    const sourceBefore = JSON.stringify(benchmark.document);
    for (const viewportWidth of PHASE2_LAYOUT_VIEWPORT_WIDTHS) {
      const label = `${benchmark.benchmarkId}@${viewportWidth}`;
      const effective = resolveResponsiveScene(benchmark.document, viewportWidth).document;
      const solved = solveSceneLayout(benchmark.document, { rootNodeId: benchmark.rootNodeId, viewportWidth });
      const root = solved.boxes.get(benchmark.rootNodeId);
      const content = solved.boxes.get(benchmark.contentNodeId);
      assert.equal(root.x, 0, `${label}: root must start at viewport origin`);
      assert.equal(root.width, viewportWidth, `${label}: root must cover the viewport width`);
      assert.ok(content.width <= Math.min(viewportWidth, 1440) + 0.01, `${label}: bounded content is wider than its viewport or max width`);
      assert.ok(Math.abs(content.x - (viewportWidth - content.width) / 2) < 0.01, `${label}: bounded content is not centered`);
      assert.deepEqual(solved.diagnostics.filter((diagnostic) => diagnostic.severity === 'error'), [], `${label}: grid solver returned an error`);
      assert.deepEqual(solved.diagnostics.filter((diagnostic) => diagnostic.code === 'overflow-x'), [], `${label}: horizontal overflow detected`);
      const clippedText = [];
      walk(effective.pages.flatMap((page) => page.children), (node) => {
        if (node.type !== 'text' || !node.visible) return;
        const box = solved.boxes.get(node.id);
        if (box && box.contentHeight > box.height + 0.01) clippedText.push(node.id);
      });
      assert.deepEqual(clippedText, [], `${label}: text was clipped`);
      assert.deepEqual(flowOverlapErrors(effective, solved), [], `${label}: flow siblings overlap`);
      if (viewportWidth >= 3840) {
        assert.equal(root.width, viewportWidth, `${label}: wide background must stay full bleed`);
        assert.equal(content.width, 1440, `${label}: wide content must respect its max width`);
      }
    }
    assert.equal(JSON.stringify(benchmark.document), sourceBefore, `${benchmark.benchmarkId}: solving mutated the source scene`);
  }
});
