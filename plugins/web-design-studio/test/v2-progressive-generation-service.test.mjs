import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { GenerationCandidateStore } from '../dist/v2-generation-candidate-store.test.mjs';
import { GenerationPlanStore } from '../dist/v2-generation-plan-store.test.mjs';
import { ProgressiveGenerationService } from '../dist/v2-progressive-generation-service.test.mjs';
import { GenerationSoftProtectionStore } from '../dist/v2-generation-soft-protection-store.test.mjs';
import { createSceneNodeBase, indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { SceneDocumentStore } from '../dist/v2-scene-store.test.mjs';

const documentId = 'website-progressive';

async function environment(mode = 'auto-current-page') {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-progressive-service-'));
  const repositories = {
    plans: new GenerationPlanStore(root),
    scenes: new SceneDocumentStore(root),
    candidates: new GenerationCandidateStore(root),
    protections: new GenerationSoftProtectionStore(root)
  };
  const service = new ProgressiveGenerationService({
    projectId: 'host-project-through-123',
    repositories,
    async assertDocumentInScope(requestedId) {
      if (requestedId !== documentId) throw new Error('Document is outside the runtime scope.');
      return { name: 'Progressive website' };
    }
  });
  const site = await service.planSite({
    documentId,
    planId: 'plan-progressive',
    mode,
    objective: 'Create a visually distinctive product website',
    audience: ['Design reviewers'],
    pages: [
      { pageId: 'home', name: '首页', purpose: '建立品牌价值' },
      { pageId: 'pricing', name: '价格', purpose: '解释购买方案' }
    ]
  });
  return { root, repositories, service, site };
}

function design() {
  return {
    artDirection: 'Editorial product composition with a restrained material language',
    compositionIntent: 'One dominant focal point followed by varied supporting sections',
    typographyIntent: 'Expressive display typography with calm readable body copy',
    imageStrategy: 'Purposeful product imagery rather than decorative stock photography',
    contentHierarchy: ['Promise', 'Evidence', 'Action'],
    designAcceptanceCriteria: ['The primary focus is unmistakable', 'The result does not resemble an admin dashboard'],
    interactionIntents: []
  };
}

function steps() {
  return [
    { stepId: 'home-structure', title: '建立首页骨架', kind: 'structure', target: { viewportWidths: [390, 1440] } },
    { stepId: 'home-design-gate', title: '首页视觉验收', kind: 'design-gate', dependsOn: ['home-structure'], target: { viewportWidths: [390, 1440] } },
    { stepId: 'home-handoff', title: '首页最终验收', kind: 'handoff', dependsOn: ['home-design-gate'], target: { viewportWidths: [390, 1440] } }
  ];
}

function visualInputs(revision) {
  return [
    { artifactId: `before-page-r${revision}`, kind: 'page-snapshot', revision, viewportWidth: 1440, uri: 'artifact://before/page', createdAt: '2026-09-08T15:00:00.000Z' },
    { artifactId: `before-grounding-r${revision}`, kind: 'visual-grounding', revision, viewportWidth: 1440, uri: 'artifact://before/grounding', createdAt: '2026-09-08T15:00:00.000Z' }
  ];
}

function verification(revision) {
  const artifact = (kind, suffix = kind) => ({
    artifactId: `candidate-${suffix}-r${revision}`, kind, revision, viewportWidth: 1440,
    uri: `artifact://candidate/${suffix}`, createdAt: '2026-09-08T15:01:00.000Z'
  });
  return {
    passed: true,
    qualitySummary: 'The page skeleton establishes a clear editorial rhythm.',
    issueIds: [],
    artifacts: [
      artifact('layout'), artifact('page-snapshot', 'page'), artifact('visual-grounding', 'grounding'),
      artifact('visual-diff', 'diff'), artifact('calibration'), artifact('quality-report', 'quality')
    ]
  };
}

function heroSection() {
  return {
    ...createSceneNodeBase('section', 'Hero section', { x: 0, y: 0, width: 1440, height: 720 }, 'ai'),
    type: 'section',
    id: 'section:hero',
    role: 'hero',
    children: []
  };
}

test('site planning stays separate from page planning and one run advances exactly one step', async () => {
  const { root, repositories, service, site } = await environment();
  try {
    assert.equal(site.plan.revision, 1);
    assert.equal(site.plan.pages.every((page) => page.status === 'unplanned'), true);
    assert.deepEqual(site.plan.nextAction, { type: 'plan-page', tool: 'web_design_plan_page', pageId: 'home' });

    const pagePlan = await service.planPage({ documentId, expectedPlanRevision: 1, pageId: 'home', design: design(), steps: steps() });
    assert.equal(pagePlan.plan.revision, 3);
    assert.equal(pagePlan.plan.status, 'ready');
    assert.equal(pagePlan.plan.pages.find((page) => page.pageId === 'pricing').status, 'unplanned');

    const started = await service.startPage(documentId, 3, 'home', 1440);
    assert.equal(started.plan.revision, 4);
    assert.equal(started.scene.revision, 2);
    assert.equal(started.scene.rootNodeId, 'root:home');

    const executed = await service.runNextStep({
      documentId,
      expectedPlanRevision: 4,
      idempotencyKey: 'home-structure:scene:2',
      transactionId: 'transaction:home-structure:1',
      operations: [{ op: 'insert-node', parentId: 'root:home', index: 0, node: heroSection() }],
      visualInputs: visualInputs(2),
      verification: verification(3)
    });
    assert.equal(executed.status, 'committed');
    assert.equal(executed.scene.revision, 3);
    assert.equal(executed.plan.pages.find((page) => page.pageId === 'home').stepCounts.accepted, 1);
    assert.equal(executed.plan.pages.find((page) => page.pageId === 'home').stepCounts.ready, 1);
    assert.equal(executed.plan.pages.find((page) => page.pageId === 'pricing').status, 'unplanned');
    const scene = await repositories.scenes.read(documentId);
    assert.ok(indexSceneDocument(scene).has('section:hero'));
    assert.equal(scene.pages.find((page) => page.id === 'pricing').children.length, 0);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('stale visual input is rejected before a plan attempt or Scene mutation is created', async () => {
  const { root, repositories, service } = await environment();
  try {
    await service.planPage({ documentId, expectedPlanRevision: 1, pageId: 'home', design: design(), steps: steps() });
    await service.startPage(documentId, 3, 'home', 1440);
    await assert.rejects(() => service.runNextStep({
      documentId,
      expectedPlanRevision: 4,
      idempotencyKey: 'stale-visual-input',
      transactionId: 'transaction:stale-visual',
      operations: [{ op: 'insert-node', parentId: 'root:home', index: 0, node: heroSection() }],
      visualInputs: visualInputs(1),
      verification: verification(3)
    }), /current Scene revision 2/);
    const scope = { projectId: 'host-project-through-123', documentId };
    assert.equal((await repositories.plans.read(scope)).revision, 4);
    assert.equal((await repositories.scenes.read(documentId)).revision, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('guided mode keeps one verified candidate outside the Scene until explicit acceptance', async () => {
  const { root, repositories, service } = await environment('guided');
  try {
    await service.planPage({ documentId, expectedPlanRevision: 1, pageId: 'home', design: design(), steps: steps() });
    await service.startPage(documentId, 3, 'home', 1440);
    const prepared = await service.runNextStep({
      documentId,
      expectedPlanRevision: 4,
      attemptId: 'attempt:guided:1',
      idempotencyKey: 'guided-home-structure',
      transactionId: 'transaction:guided-home-structure',
      operations: [{ op: 'insert-node', parentId: 'root:home', index: 0, node: heroSection() }],
      visualInputs: visualInputs(2),
      verification: verification(3)
    });
    assert.equal(prepared.status, 'awaiting-review');
    assert.equal((await repositories.scenes.read(documentId)).revision, 2);
    const inspected = await service.inspectStep(documentId, 'home-structure');
    assert.equal(inspected.candidate.attemptId, 'attempt:guided:1');
    const accepted = await service.acceptStep(documentId, prepared.plan.revision, 'home-structure', 'attempt:guided:1');
    assert.equal(accepted.status, 'committed');
    assert.equal((await repositories.scenes.read(documentId)).revision, 3);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('rollback restores the exact latest Scene transaction and marks dependent steps stale', async () => {
  const { root, repositories, service } = await environment();
  try {
    await service.planPage({ documentId, expectedPlanRevision: 1, pageId: 'home', design: design(), steps: steps() });
    await service.startPage(documentId, 3, 'home', 1440);
    const executed = await service.runNextStep({
      documentId,
      expectedPlanRevision: 4,
      idempotencyKey: 'rollback-home-structure',
      transactionId: 'transaction:rollback-home-structure',
      operations: [{ op: 'insert-node', parentId: 'root:home', index: 0, node: heroSection() }],
      visualInputs: visualInputs(2),
      verification: verification(3)
    });
    const rolledBack = await service.rollbackStep(documentId, executed.plan.revision, 'home-structure');
    assert.equal(rolledBack.status, 'rolled-back');
    assert.equal(rolledBack.scene.revision, 4);
    assert.equal(rolledBack.recovered, false);
    const scene = await repositories.scenes.read(documentId);
    assert.equal(indexSceneDocument(scene).has('section:hero'), false);
    const plan = await repositories.plans.read({ projectId: 'host-project-through-123', documentId });
    assert.equal(plan.pageRuns[0].steps.find((step) => step.stepId === 'home-structure').status, 'rolled-back');
    assert.equal(plan.pageRuns[0].steps.find((step) => step.stepId === 'home-design-gate').status, 'stale');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
