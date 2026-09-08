import assert from 'node:assert/strict';
import test from 'node:test';
import { solveSceneLayout } from '../dist/v2-layout-engine.test.mjs';
import { evaluateVisualQuality, createVisualRepairRequest } from '../dist/v2-visual-quality.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

function protocol() {
  const scope = { projectId: 'project-quality', documentId: 'scene-test' };
  const brief = {
    schemaVersion: 1, briefId: 'brief-quality', scope,
    objective: 'Validate a responsive hero.', audience: ['Product teams'],
    brand: { name: 'Arc', attributes: ['precise', 'calm'], visualReferences: [] },
    content: { locale: 'en', pages: [{ pageKey: 'home', name: 'Home', purpose: 'Explain value', sections: [{ sectionKey: 'hero', role: 'hero-heading', goal: 'State value', contentRequirements: ['headline'] }] }] },
    constraints: { viewportWidths: [390, 1440], accessibilityLevel: 'AA', forbiddenPatterns: ['horizontal overflow'] },
    createdAt: '2026-09-08T03:00:00.000Z'
  };
  const spec = {
    schemaVersion: 1, specId: 'spec-quality', briefId: brief.briefId, scope, sourceRevision: 0,
    designPrinciples: ['Readable hierarchy', 'Continuous response'],
    tokenIntents: [{ tokenKey: 'space-page', type: 'number', purpose: 'Page spacing', modes: ['default'] }],
    pages: [{ pageKey: 'home', sections: [{
      sectionKey: 'hero', nodeRole: 'hero-heading', visualPriority: 'primary',
      layoutIntent: { mode: 'auto', direction: 'vertical', wrap: false, widthBehavior: 'bounded', maxContentWidth: 1440 },
      componentStrategy: { source: 'native', preferredLibraries: [], requiredInteractions: [] }, responsiveIntent: ['Wrap text continuously']
    }] }],
    globalResponsiveIntent: ['No horizontal overflow'], createdAt: '2026-09-08T03:01:00.000Z'
  };
  return { brief, spec };
}

test('visual quality passes a complete scene with fresh layouts at every required viewport', () => {
  const document = nestedWebsite();
  const { brief, spec } = protocol();
  const layouts = brief.constraints.viewportWidths.map((viewportWidth) => solveSceneLayout(document, { rootNodeId: 'frame-desktop', viewportWidth }));
  const report = evaluateVisualQuality(document, brief, spec, { layouts });
  assert.equal(report.passed, true);
  assert.equal(report.score, 100);
  assert.deepEqual(report.evaluatedViewportWidths, [390, 1440]);
  assert.deepEqual(report.issues, []);
});

test('visual quality reports missing semantics, layout diagnostics, and browser calibration by stable node id', () => {
  const document = nestedWebsite();
  const { brief, spec } = protocol();
  spec.pages[0].sections[0].nodeRole = 'missing-checkout';
  const layouts = brief.constraints.viewportWidths.map((viewportWidth) => {
    const layout = solveSceneLayout(document, { rootNodeId: 'frame-desktop', viewportWidth });
    if (viewportWidth === 390) layout.diagnostics.push({ nodeId: 'frame-desktop', severity: 'warning', code: 'overflow-x', message: 'Content exceeds frame width.' });
    return layout;
  });
  const calibrations = [{
    rootNodeId: 'frame-desktop', viewportWidth: 390, tolerance: 1, passed: false, comparedNodeCount: 1,
    issues: [{ code: 'geometry-mismatch', nodeId: 'group-hero-copy', severity: 'error', expected: { x: 0, y: 0, width: 10, height: 10 }, actual: { x: 2, y: 0, width: 10, height: 10 }, delta: { x: 2, y: 0, width: 0, height: 0 }, maximumDelta: 2 }]
  }];
  const report = evaluateVisualQuality(document, brief, spec, { layouts, calibrations });
  assert.equal(report.passed, false);
  assert.ok(report.issues.some((issue) => issue.criterion === 'semantic-completeness' && issue.severity === 'blocker'));
  assert.ok(report.issues.some((issue) => issue.nodeIds[0] === 'frame-desktop' && issue.viewportWidth === 390));
  assert.ok(report.issues.some((issue) => issue.nodeIds[0] === 'group-hero-copy' && issue.criterion === 'rendering'));
});

test('repair requests target only repairable nodes and block human-protected work', () => {
  const document = nestedWebsite();
  const { brief, spec } = protocol();
  const layouts = brief.constraints.viewportWidths.map((viewportWidth) => {
    const layout = solveSceneLayout(document, { rootNodeId: 'frame-desktop', viewportWidth });
    layout.diagnostics.push({ nodeId: 'frame-desktop', severity: 'warning', code: 'overflow-x', message: 'Frame overflow.' });
    layout.diagnostics.push({ nodeId: 'text-hero-heading', severity: 'warning', code: 'overflow-y', message: 'Heading overflow.' });
    return layout;
  });
  const report = evaluateVisualQuality(document, brief, spec, { layouts });
  const request = createVisualRepairRequest(report, document);
  assert.deepEqual(request.targetNodeIds, ['frame-desktop']);
  assert.deepEqual(request.protectedNodeIds, ['text-hero-heading']);
  assert.equal(request.issueIds.length, 2);
  assert.equal(request.blockedIssueIds.length, 2);
  assert.equal(request.constraints.preserveHumanLocks, true);
});

test('visual quality rejects stale layout and repair artifacts', () => {
  const document = nestedWebsite();
  const { brief, spec } = protocol();
  const stale = solveSceneLayout(document, { rootNodeId: 'frame-desktop', viewportWidth: 390 });
  stale.revision = 99;
  assert.throws(() => evaluateVisualQuality(document, brief, spec, { layouts: [stale] }), /stale/);
  const layouts = brief.constraints.viewportWidths.map((viewportWidth) => solveSceneLayout(document, { rootNodeId: 'frame-desktop', viewportWidth }));
  const report = evaluateVisualQuality(document, brief, spec, { layouts });
  document.revision += 1;
  assert.throws(() => createVisualRepairRequest(report, document), /stale/);
});
