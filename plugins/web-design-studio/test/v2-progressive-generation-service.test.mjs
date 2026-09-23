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
    },
    async verifyCandidate({ candidateDocument }) {
      return { ...verification(candidateDocument.revision), __images: [{ label: 'candidate', data: 'cG5n', mimeType: 'image/png' }] };
    },
    async captureVisualInputs({ revision }) {
      return visualInputs(revision);
    },
    async loadArtifactImages() {
      return [{ label: 'stored-candidate', data: 'cG5n', mimeType: 'image/png' }];
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
  return [390, 1440].flatMap((viewportWidth) => [
    { artifactId: `before-page-${viewportWidth}-r${revision}`, kind: 'page-snapshot', revision, viewportWidth, uri: 'artifact://before/page', createdAt: '2026-09-08T15:00:00.000Z' },
    { artifactId: `before-grounding-${viewportWidth}-r${revision}`, kind: 'visual-grounding', revision, viewportWidth, uri: 'artifact://before/grounding', createdAt: '2026-09-08T15:00:00.000Z' }
  ]);
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
    assert.equal(site.plan.deliveryGate.code, 'NO_ACCEPTED_VISUAL_STEP');
    assert.equal(site.plan.deliveryGate.visibleSceneReady, false);
    assert.equal(site.plan.deliveryGate.projectImplementationAllowed, false);
    assert.equal(site.plan.deliveryGate.taskCompletionAllowed, false);
    assert.deepEqual(site.plan.deliveryGate.requiredNextAction, site.plan.nextAction);

    const pagePlan = await service.planPage({ documentId, expectedPlanRevision: 1, pageId: 'home', design: design(), steps: steps() });
    assert.equal(pagePlan.plan.revision, 4);
    assert.equal(pagePlan.plan.status, 'running');
    assert.equal(pagePlan.plan.pages.find((page) => page.pageId === 'pricing').status, 'unplanned');
    assert.equal(pagePlan.scene.revision, 2);
    assert.equal(pagePlan.scene.rootNodeId, 'root:home');
    const startedScene = await repositories.scenes.read(documentId);
    const startedRoot = startedScene.pages.find((page) => page.id === 'home').children[0];
    assert.equal(startedRoot.layout.sizingX, 'fill');
    assert.equal(startedRoot.layout.sizingY, 'hug');
    assert.equal(startedRoot.layout.minHeight, 1);

    const executed = await service.executeStep({
      documentId,
      expectedPlanRevision: 4,
      requestId: 'home-structure-1',
      operations: [{ op: 'insert-node', parentId: 'root:home', index: 0, node: heroSection() }]
    });
    assert.equal(executed.status, 'awaiting-review');
    assert.equal(executed.__images[0].label, 'candidate');
    assert.ok(executed.candidate.reviewArtifacts.length > 0);
    assert.equal(Object.hasOwn(executed.candidate, 'artifacts'), false);
    assert.equal(Object.hasOwn(executed.candidate, 'nextVisualInputs'), false);
    const resumed = await service.getActiveContext(documentId);
    assert.equal(resumed.resumeReview.candidate.attemptId, executed.candidate.attemptId);
    assert.equal(resumed.__images[0].label, 'stored-candidate');
    assert.equal((await repositories.scenes.read(documentId)).revision, 2);
    const accepted = await service.acceptStep(documentId, executed.plan.revision, 'home-structure', executed.candidate.attemptId);
    assert.equal(accepted.status, 'committed');
    assert.equal(Object.hasOwn(accepted, 'transaction'), false);
    assert.ok(accepted.change.affectedNodeIds.includes('section:hero'));
    assert.equal(accepted.scene.revision, 3);
    assert.equal(accepted.plan.pages.find((page) => page.pageId === 'home').stepCounts.accepted, 1);
    assert.equal(accepted.plan.pages.find((page) => page.pageId === 'home').stepCounts.ready, 1);
    assert.equal(accepted.plan.pages.find((page) => page.pageId === 'pricing').status, 'unplanned');
    assert.equal(accepted.plan.deliveryGate.code, 'NO_COMPLETED_ARTBOARD');
    assert.equal(accepted.plan.deliveryGate.visibleSceneReady, true);
    assert.equal(accepted.plan.deliveryGate.projectImplementationAllowed, false);
    assert.equal(accepted.plan.deliveryGate.acceptedVisibleStepCount, 1);
    const scene = await repositories.scenes.read(documentId);
    assert.ok(indexSceneDocument(scene).has('section:hero'));
    assert.equal(scene.pages.find((page) => page.id === 'pricing').children.length, 0);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('accepting handoff automatically completes the artboard and returns a compact checkpoint', async () => {
  const { root, service } = await environment();
  try {
    const planned = await service.planPage({ documentId, expectedPlanRevision: 1, pageId: 'home', design: design(), steps: steps() });
    let planRevision = planned.plan.revision;

    const structure = await service.executeStep({
      documentId, expectedPlanRevision: planRevision, requestId: 'complete-structure',
      operations: [{ op: 'insert-node', parentId: 'root:home', index: 0, node: heroSection() }]
    });
    const structureAccepted = await service.acceptStep(documentId, structure.plan.revision, 'home-structure', structure.candidate.attemptId);
    planRevision = structureAccepted.plan.revision;

    const gate = await service.executeStep({
      documentId, expectedPlanRevision: planRevision, requestId: 'complete-gate',
      operations: [{ op: 'update-node', nodeId: 'root:home', patches: [{ path: ['name'], value: 'Approved home artboard' }] }]
    });
    const gateAccepted = await service.acceptStep(documentId, gate.plan.revision, 'home-design-gate', gate.candidate.attemptId);
    planRevision = gateAccepted.plan.revision;

    const handoff = await service.executeStep({
      documentId, expectedPlanRevision: planRevision, requestId: 'complete-handoff',
      operations: [{ op: 'update-node', nodeId: 'root:home', patches: [{ path: ['name'], value: 'Delivered home artboard' }] }]
    });
    const completed = await service.acceptStep(documentId, handoff.plan.revision, 'home-handoff', handoff.candidate.attemptId);
    assert.equal(completed.status, 'page-completed');
    assert.equal(completed.plan.pages.find((page) => page.pageId === 'home').status, 'completed');
    assert.equal(completed.contextCheckpoint.kind, 'artboard-complete');
    assert.equal(completed.contextCheckpoint.pageId, 'home');
    assert.equal(Object.hasOwn(completed, 'transaction'), false);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('the service captures current-revision visual input automatically', async () => {
  const { root, repositories, service } = await environment();
  try {
    const planned = await service.planPage({ documentId, expectedPlanRevision: 1, pageId: 'home', design: design(), steps: steps() });
    const executed = await service.executeStep({
      documentId,
      expectedPlanRevision: planned.plan.revision,
      requestId: 'automatic-visual-input',
      operations: [{ op: 'insert-node', parentId: 'root:home', index: 0, node: heroSection() }]
    });
    assert.equal(executed.status, 'awaiting-review');
    const scope = { projectId: 'host-project-through-123', documentId };
    assert.equal((await repositories.plans.read(scope)).revision, executed.plan.revision);
    assert.equal((await repositories.scenes.read(documentId)).revision, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('guided mode keeps one verified candidate outside the Scene until explicit acceptance', async () => {
  const { root, repositories, service } = await environment('guided');
  try {
    const planned = await service.planPage({ documentId, expectedPlanRevision: 1, pageId: 'home', design: design(), steps: steps() });
    const prepared = await service.executeStep({
      documentId,
      expectedPlanRevision: planned.plan.revision,
      requestId: 'guided-home-structure',
      operations: [{ op: 'insert-node', parentId: 'root:home', index: 0, node: heroSection() }]
    });
    assert.equal(prepared.status, 'awaiting-review');
    assert.equal((await repositories.scenes.read(documentId)).revision, 2);
    const inspected = await service.inspectStep(documentId, 'home-structure');
    assert.equal(inspected.candidate.attemptId, 'attempt:guided-home-structure');
    assert.equal(inspected.__images[0].label, 'stored-candidate');
    const accepted = await service.acceptStep(documentId, prepared.plan.revision, 'home-structure', 'attempt:guided-home-structure');
    assert.equal(accepted.status, 'committed');
    assert.equal((await repositories.scenes.read(documentId)).revision, 3);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('rollback restores the exact latest Scene transaction and marks dependent steps stale', async () => {
  const { root, repositories, service } = await environment();
  try {
    const planned = await service.planPage({ documentId, expectedPlanRevision: 1, pageId: 'home', design: design(), steps: steps() });
    const executed = await service.executeStep({
      documentId,
      expectedPlanRevision: planned.plan.revision,
      requestId: 'rollback-home-structure',
      operations: [{ op: 'insert-node', parentId: 'root:home', index: 0, node: heroSection() }]
    });
    const accepted = await service.acceptStep(documentId, executed.plan.revision, 'home-structure', executed.candidate.attemptId);
    const rolledBack = await service.rollbackStep(documentId, accepted.plan.revision, 'home-structure');
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
