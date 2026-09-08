import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { createBlankSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { SceneDocumentStore } from '../dist/v2-scene-store.test.mjs';
import { createDesignGenerationPlan } from '../dist/v2-design-protocol.test.mjs';
import { executeDesignGenerationPlan } from '../dist/v2-design-generation-executor.test.mjs';
import { executeDesignRepair } from '../dist/v2-design-repair-executor.test.mjs';
import { createPhase2LayoutBenchmarks } from '../dist/v2-phase2-layout-benchmarks.test.mjs';
import { solveSceneLayout } from '../dist/v2-layout-engine.test.mjs';
import { calibrateSceneLayout } from '../dist/v2-layout-calibration.test.mjs';
import { renderPhase2BenchmarkScene } from '../dist/v2-scene-html-renderer.test.mjs';
import { createVisualRepairRequest, evaluateVisualQuality } from '../dist/v2-visual-quality.test.mjs';

const benchmarkIds = ['saas-product', 'consumer-commerce', 'editorial-magazine'];

function protocolFor(benchmark, sourceRevision) {
  const root = benchmark.document.pages[0].children[0];
  const content = root.children[0];
  const scope = { projectId: `project-${benchmark.benchmarkId}`, documentId: benchmark.document.documentId };
  const sections = content.children.map((node, index) => ({
    sectionKey: `section-${index + 1}`,
    role: node.role,
    goal: `Create the ${node.role} region as an independently reviewable semantic section.`,
    contentRequirements: [`Complete ${node.role} content`]
  }));
  const brief = {
    schemaVersion: 1,
    briefId: `brief-${benchmark.benchmarkId}`,
    scope,
    objective: `Generate a production ${benchmark.benchmarkId} website from semantic intent.`,
    audience: ['Website visitors'],
    brand: { name: benchmark.benchmarkId, attributes: ['distinctive', 'usable'], visualReferences: [] },
    content: { locale: 'zh-CN', pages: [{ pageKey: 'home', name: 'Home', purpose: 'Present the complete website', sections }] },
    constraints: { viewportWidths: [390, 1440], accessibilityLevel: 'AA', forbiddenPatterns: ['horizontal overflow'] },
    createdAt: '2026-09-08T06:00:00.000Z'
  };
  const spec = {
    schemaVersion: 1,
    specId: `spec-${benchmark.benchmarkId}`,
    briefId: brief.briefId,
    scope,
    sourceRevision,
    designPrinciples: ['Preserve semantic hierarchy', 'Respond continuously without duplicate device trees'],
    tokenIntents: [{ tokenKey: 'surface-canvas', type: 'color', purpose: 'Canvas surface color', modes: ['default'] }],
    pages: [{
      pageKey: 'home',
      sections: sections.map((section) => ({
        sectionKey: section.sectionKey,
        nodeRole: section.role,
        visualPriority: section.sectionKey === 'section-2' ? 'primary' : 'secondary',
        layoutIntent: { mode: 'auto', direction: 'vertical', wrap: false, widthBehavior: 'bounded', maxContentWidth: 1440 },
        componentStrategy: { source: 'native', preferredLibraries: [], requiredInteractions: [] },
        responsiveIntent: ['Use the same semantic node tree at mobile and desktop widths']
      }))
    }],
    globalResponsiveIntent: ['No horizontal overflow at any required viewport'],
    createdAt: '2026-09-08T06:01:00.000Z'
  };
  return { brief, spec, root, content };
}

function variableCollection(benchmarkId) {
  return {
    id: `variables-${benchmarkId}`,
    name: 'Generated design system',
    modes: [{ id: 'mode-default', name: 'Default' }],
    variables: [{ id: `variable-${benchmarkId}-surface`, name: 'Canvas surface', type: 'color', valuesByMode: { 'mode-default': '#FFFFFF' } }]
  };
}

function exactCalibration(layout) {
  const measurements = {};
  for (const [nodeId, box] of layout.boxes) {
    measurements[nodeId] = {
      rect: { x: box.x, y: box.y, width: box.width, height: box.height },
      scrollWidth: box.width,
      scrollHeight: box.height
    };
  }
  return calibrateSceneLayout(layout, measurements, 0.01);
}

function snapshotsFor(benchmark, document, viewportWidths) {
  return viewportWidths.map((viewportWidth) => {
    const rendered = renderPhase2BenchmarkScene({ ...benchmark, document }, viewportWidth);
    return {
      viewportWidth,
      snapshotId: `snapshot:${benchmark.benchmarkId}:${document.revision}:${viewportWidth}`,
      sha256: createHash('sha256').update(rendered.html).digest('hex')
    };
  });
}

async function runWebsite(benchmark) {
  const directory = await mkdtemp(join(tmpdir(), `web-design-e2e-${benchmark.benchmarkId}-`));
  try {
    const store = new SceneDocumentStore(directory);
    const blank = createBlankSceneDocument(`Generated ${benchmark.benchmarkId}`);
    blank.documentId = benchmark.document.documentId;
    blank.pages[0].id = benchmark.document.pages[0].id;
    blank.pages[0].name = benchmark.document.pages[0].name;
    const created = await store.create(blank);
    assert.deepEqual(created.pages[0].children, []);
    const { brief, spec, root: sourceRoot, content: sourceContent } = protocolFor(benchmark, created.revision);
    const plan = createDesignGenerationPlan(brief, spec);
    const sectionSteps = plan.steps.filter((step) => step.kind === 'apply-section-transaction');
    const run = await executeDesignGenerationPlan(plan, store, {
      createTransaction(step, document, scope) {
        assert.deepEqual(scope, brief.scope);
        if (step.kind === 'create-design-system') {
          return {
            transactionId: `${benchmark.benchmarkId}-design-system`, baseRevision: document.revision, author: 'ai',
            operations: [{ op: 'insert-variable-collection', index: 0, collection: variableCollection(benchmark.benchmarkId) }]
          };
        }
        const sectionIndex = sectionSteps.findIndex((candidate) => candidate.stepId === step.stepId);
        const sourceSection = structuredClone(sourceContent.children[sectionIndex]);
        const operations = [];
        if (sectionIndex === 0) {
          const root = structuredClone(sourceRoot);
          const content = structuredClone(sourceContent);
          content.children = [sourceSection];
          content.layout.sizingX = 'fixed';
          content.frame.width = 1600;
          root.children = [content];
          operations.push({ op: 'insert-node', parentId: document.pages[0].id, index: 0, node: root });
        } else {
          operations.push({ op: 'insert-node', parentId: sourceContent.id, index: sectionIndex, node: sourceSection });
        }
        if (sectionIndex === sectionSteps.length - 1) {
          benchmark.document.responsiveRules.forEach((rule, index) => operations.push({ op: 'insert-responsive-rule', index, rule: structuredClone(rule) }));
        }
        return { transactionId: `${benchmark.benchmarkId}-section-${sectionIndex + 1}`, baseRevision: document.revision, author: 'ai', operations };
      },
      solveLayout(step, document, scope) {
        assert.deepEqual(scope, brief.scope);
        return step.viewportWidths.map((viewportWidth) => {
          const layout = solveSceneLayout(document, { rootNodeId: sourceRoot.id, viewportWidth });
          return {
            viewportWidth, rootNodeId: sourceRoot.id,
            errorDiagnostics: layout.diagnostics.filter((diagnostic) => diagnostic.severity === 'error').length,
            warningDiagnostics: layout.diagnostics.filter((diagnostic) => diagnostic.severity === 'warning').length
          };
        });
      },
      renderSnapshots(step, document, scope) {
        assert.deepEqual(scope, brief.scope);
        return snapshotsFor(benchmark, document, step.viewportWidths);
      },
      critique(_step, document, scope) {
        assert.deepEqual(scope, brief.scope);
        const issues = brief.constraints.viewportWidths.flatMap((viewportWidth) => solveSceneLayout(document, { rootNodeId: sourceRoot.id, viewportWidth }).diagnostics);
        return issues.length > 0
          ? { verdict: 'revise', issueIds: issues.map((issue, index) => `${issue.nodeId}:${issue.code}:${index}`), summary: 'Responsive diagnostics require a targeted repair.' }
          : { verdict: 'pass', issueIds: [], summary: 'All required viewports pass.' };
      }
    });
    assert.equal(run.status, 'completed');
    assert.equal(run.steps.filter((step) => step.transactionSummary).length, sectionSteps.length + 1);
    assert.equal(run.steps.at(-1).critique.verdict, 'revise');

    const generated = await store.read(brief.scope.documentId);
    assert.equal(generated.pages[0].children.length, 1);
    assert.deepEqual(generated.responsiveRules.map((rule) => rule.id), benchmark.document.responsiveRules.map((rule) => rule.id));
    const initialLayouts = brief.constraints.viewportWidths.map((viewportWidth) => solveSceneLayout(generated, { rootNodeId: sourceRoot.id, viewportWidth }));
    const initialReport = evaluateVisualQuality(generated, brief, spec, {
      layouts: initialLayouts,
      calibrations: initialLayouts.map(exactCalibration)
    });
    assert.equal(initialReport.passed, false);
    assert.ok(initialReport.issues.some((issue) => issue.criterion === 'responsive' && issue.nodeIds.includes(sourceRoot.id)));
    const request = createVisualRepairRequest(initialReport, generated);
    assert.ok(request.targetNodeIds.includes(sourceRoot.id));

    const repair = await executeDesignRepair(request, initialReport, brief, spec, store, {
      createTransaction(repairRequest, _report, document, scope) {
        assert.deepEqual(scope, brief.scope);
        assert.ok(repairRequest.targetNodeIds.includes(sourceRoot.id));
        return {
          transactionId: `${benchmark.benchmarkId}-responsive-repair`, baseRevision: document.revision, author: 'ai',
          operations: [{ op: 'update-node', nodeId: sourceContent.id, patches: [{ path: ['layout', 'sizingX'], value: 'fill' }, { path: ['frame', 'width'], value: sourceContent.frame.width }] }]
        };
      },
      verify(document, viewportWidths, scope) {
        assert.deepEqual(scope, brief.scope);
        const layouts = viewportWidths.map((viewportWidth) => solveSceneLayout(document, { rootNodeId: sourceRoot.id, viewportWidth }));
        return {
          layouts,
          snapshots: snapshotsFor(benchmark, document, viewportWidths),
          calibrations: layouts.map(exactCalibration)
        };
      }
    });
    assert.equal(repair.status, 'completed');
    assert.equal(repair.qualityReport.passed, true);
    assert.ok(repair.qualityReport.issues.every((issue) => issue.severity === 'warning'));
    assert.ok(repair.snapshots.every((snapshot) => /^[a-f0-9]{64}$/.test(snapshot.sha256)));
    return {
      benchmarkId: benchmark.benchmarkId,
      structuralSignature: benchmark.structuralSignature,
      sectionRoles: sourceContent.children.map((node) => node.role),
      generationRevision: run.finalRevision,
      finalRevision: repair.finalRevision
    };
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

test('three structurally different websites complete empty-scene generation, critique, scoped repair, and visual revalidation', async () => {
  const catalog = createPhase2LayoutBenchmarks().filter((benchmark) => benchmarkIds.includes(benchmark.benchmarkId));
  assert.equal(catalog.length, 3);
  const results = [];
  for (const benchmark of catalog) results.push(await runWebsite(benchmark));
  assert.equal(new Set(results.map((result) => result.structuralSignature)).size, 3);
  assert.equal(new Set(results.map((result) => result.sectionRoles.join('|'))).size, 3);
  assert.ok(results.every((result) => result.finalRevision === result.generationRevision + 1));
});
