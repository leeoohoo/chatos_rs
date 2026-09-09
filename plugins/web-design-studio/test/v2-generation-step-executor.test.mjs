import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { GenerationCandidateStore } from '../dist/v2-generation-candidate-store.test.mjs';
import { GenerationPlanStore } from '../dist/v2-generation-plan-store.test.mjs';
import { GenerationSoftProtectionStore } from '../dist/v2-generation-soft-protection-store.test.mjs';
import { commitGenerationStep, prepareGenerationStep, validateGenerationStepTransaction } from '../dist/v2-generation-step-executor.test.mjs';
import { indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { SceneDocumentStore } from '../dist/v2-scene-store.test.mjs';
import { generationExecutorPlan } from './helpers/v2-generation-executor-fixture.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

const scope = { projectId: 'project-candidate', documentId: 'scene-test' };
const timestamp = '2026-09-08T12:00:00.000Z';

async function environment() {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-step-executor-'));
  const repositories = {
    scenes: new SceneDocumentStore(path.join(root, 'scenes')),
    plans: new GenerationPlanStore(path.join(root, 'plans')),
    candidates: new GenerationCandidateStore(path.join(root, 'candidates'))
  };
  await repositories.scenes.create(nestedWebsite());
  await repositories.plans.create(generationExecutorPlan());
  await repositories.plans.apply(scope, 1, { type: 'mark-ready' }, timestamp);
  await repositories.plans.apply(scope, 2, { type: 'start-page', pageId: 'page-home' }, timestamp);
  return { root, repositories };
}

function transaction(fontSize = 72, baseRevision = 1) {
  return {
    transactionId: `candidate-font-${fontSize}`,
    baseRevision,
    author: 'ai',
    operations: [{
      op: 'update-node', nodeId: 'text-hero-heading',
      patches: [{ path: ['appearance', 'typography', 'fontSize'], value: fontSize }]
    }]
  };
}

function artifacts(revision = 2) {
  return [
    { artifactId: `snapshot-r${revision}`, kind: 'page-snapshot', revision, viewportWidth: 1440, uri: 'artifact://snapshot', createdAt: timestamp },
    { artifactId: `grounding-r${revision}`, kind: 'visual-grounding', revision, viewportWidth: 1440, uri: 'artifact://grounding', createdAt: timestamp },
    { artifactId: `quality-r${revision}`, kind: 'quality-report', revision, uri: 'artifact://quality', createdAt: timestamp }
  ];
}

function passingHandlers(fontSize = 72) {
  return {
    generate({ document }) { return transaction(fontSize, document.revision); },
    verify({ candidateDocument }) {
      assert.equal(indexSceneDocument(candidateDocument).get('text-hero-heading').node.appearance.typography.fontSize, fontSize);
      return { passed: true, artifacts: artifacts(candidateDocument.revision), qualitySummary: 'The visual hierarchy passes.', issueIds: [] };
    }
  };
}

function prepareInput() {
  return {
    scope,
    expectedPlanRevision: 3,
    pageId: 'page-home',
    stepId: 'home-visual',
    attemptId: 'attempt-home-visual-1',
    idempotencyKey: 'home-visual:scene:1'
  };
}

test('generation keeps prototype wiring inside post-design interaction steps', () => {
  const document = nestedWebsite();
  document.pages.push({ id: 'page-details', name: 'Details', children: [] });
  const page = { pageId: 'page-home', name: 'Home', purpose: 'Landing', order: 0, status: 'running', steps: [], attempts: [], createdAt: timestamp, updatedAt: timestamp };
  const target = { sectionKey: 'hero', nodeIds: ['group-hero-copy'], viewportWidths: [1440] };
  const visualStep = { stepId: 'visual-step', pageId: 'page-home', title: 'Visual hierarchy', kind: 'visual', required: true, dependsOn: [], target, status: 'ready', attempts: [], createdAt: timestamp, updatedAt: timestamp };
  const linkTransaction = {
    transactionId: 'candidate-prototype-link', baseRevision: document.revision, author: 'ai',
    operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{
      path: ['prototypeLink'], value: { trigger: 'click', action: 'navigate', targetPageId: 'page-details' }
    }] }]
  };
  assert.throws(() => validateGenerationStepTransaction(document, page, visualStep, linkTransaction), /interaction step/i);

  const interactionStep = { ...visualStep, stepId: 'interaction-step', title: 'Link details', kind: 'interaction' };
  assert.doesNotThrow(() => validateGenerationStepTransaction(document, page, interactionStep, linkTransaction));
  assert.throws(() => validateGenerationStepTransaction(document, page, interactionStep, transaction(72, document.revision)), /only prototype links/i);
});

test('verified candidate stays outside the formal Scene until explicit commit', async () => {
  const { root, repositories } = await environment();
  try {
    const prepared = await prepareGenerationStep(prepareInput(), repositories, passingHandlers(), timestamp);
    assert.equal(prepared.status, 'prepared');
    assert.equal(prepared.plan.revision, 6);
    assert.equal(prepared.plan.pageRuns[0].steps[0].status, 'awaiting-review');
    assert.equal((await repositories.scenes.read(scope.documentId)).revision, 1);
    assert.equal(indexSceneDocument(await repositories.scenes.read(scope.documentId)).get('text-hero-heading').node.appearance.typography.fontSize, 64);

    const committed = await commitGenerationStep({ ...prepareInput(), expectedPlanRevision: 6 }, repositories, timestamp);
    assert.equal(committed.status, 'committed');
    assert.equal(committed.document.revision, 2);
    assert.equal(committed.plan.pageRuns[0].steps[0].status, 'accepted');
    assert.equal(indexSceneDocument(committed.document).get('text-hero-heading').node.appearance.typography.fontSize, 72);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('generation, candidate application, render, and quality failures never change the Scene revision', async (t) => {
  const cases = [
    {
      name: 'generation',
      handlers: { generate() { throw new Error('model output failed'); }, verify() { throw new Error('must not verify'); } },
      code: 'generation_error'
    },
    {
      name: 'candidate application',
      handlers: {
        generate({ document }) {
          return { ...transaction(72, document.revision), transactionId: 'candidate-hard-lock', operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['content'], value: 'Forbidden overwrite' }] }] };
        },
        verify() { throw new Error('must not verify'); }
      },
      code: 'scope_violation'
    },
    {
      name: 'render',
      handlers: { generate({ document }) { return transaction(72, document.revision); }, verify() { throw new Error('snapshot renderer failed'); } },
      code: 'render_error'
    },
    {
      name: 'layout',
      handlers: {
        generate({ document }) { return transaction(72, document.revision); },
        verify() { return { passed: false, artifacts: [], qualitySummary: 'Layout overflow.', issueIds: ['overflow-x'], error: { code: 'layout_error', message: 'Layout overflow.', retryable: true, issueIds: ['overflow-x'] } }; }
      },
      code: 'layout_error'
    },
    {
      name: 'quality',
      handlers: {
        generate({ document }) { return transaction(72, document.revision); },
        verify() { return { passed: false, artifacts: artifacts(), qualitySummary: 'The page still looks generic.', issueIds: ['generic-layout'] }; }
      },
      code: 'quality_reject'
    }
  ];

  for (const item of cases) {
    await t.test(item.name, async () => {
      const { root, repositories } = await environment();
      try {
        const result = await prepareGenerationStep(prepareInput(), repositories, item.handlers, timestamp);
        assert.equal(result.status, 'failed');
        assert.equal(result.error.code, item.code);
        assert.equal((await repositories.scenes.read(scope.documentId)).revision, 1);
        assert.equal(indexSceneDocument(await repositories.scenes.read(scope.documentId)).get('text-hero-heading').node.appearance.typography.fontSize, 64);
      } finally {
        await rm(root, { recursive: true, force: true });
      }
    });
  }
});

test('a Scene change after visual verification makes the candidate stale instead of overwriting human work', async () => {
  const { root, repositories } = await environment();
  try {
    const prepared = await prepareGenerationStep(prepareInput(), repositories, passingHandlers(), timestamp);
    assert.equal(prepared.status, 'prepared');
    await repositories.scenes.apply(scope.documentId, {
      transactionId: 'human-change-after-candidate', baseRevision: 1, author: 'human',
      operations: [{ op: 'update-node', nodeId: 'group-hero-copy', patches: [{ path: ['frame', 'x'], value: 120 }] }]
    });
    const result = await commitGenerationStep({ ...prepareInput(), expectedPlanRevision: 6 }, repositories, timestamp);
    assert.equal(result.status, 'stale');
    assert.equal(result.currentSceneRevision, 2);
    assert.equal(result.plan.pageRuns[0].steps[0].status, 'stale');
    const scene = await repositories.scenes.read(scope.documentId);
    assert.equal(indexSceneDocument(scene).get('group-hero-copy').node.frame.x, 120);
    assert.equal(indexSceneDocument(scene).get('text-hero-heading').node.appearance.typography.fontSize, 64);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('soft-protected human fields require explicit review before candidate commit', async () => {
  const { root, repositories } = await environment();
  try {
    const humanTransaction = {
      transactionId: 'human-font-tuning', baseRevision: 1, author: 'human',
      operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['appearance', 'typography', 'fontSize'], value: 68 }] }]
    };
    await repositories.scenes.apply(scope.documentId, humanTransaction);
    const protectionStore = new GenerationSoftProtectionStore(path.join(root, 'protections'));
    await protectionStore.recordHumanTransaction(scope, humanTransaction, 2, 'The human manually tuned the heading scale.');
    repositories.protections = protectionStore;
    const prepared = await prepareGenerationStep(prepareInput(), repositories, passingHandlers(80), timestamp);
    assert.equal(prepared.status, 'prepared');
    assert.equal(prepared.candidate.protectionConflicts.length, 1);
    const blocked = await commitGenerationStep({ ...prepareInput(), expectedPlanRevision: 6 }, repositories, timestamp);
    assert.equal(blocked.status, 'requires-protection-review');
    assert.equal((await repositories.scenes.read(scope.documentId)).revision, 2);
    const accepted = await commitGenerationStep({ ...prepareInput(), expectedPlanRevision: 6, approveSoftProtectionConflicts: true }, repositories, timestamp);
    assert.equal(accepted.status, 'committed');
    assert.equal(indexSceneDocument(accepted.document).get('text-hero-heading').node.appearance.typography.fontSize, 80);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('commit recovers when the Scene transaction succeeded before the Plan acknowledgement', async () => {
  const { root, repositories } = await environment();
  try {
    const prepared = await prepareGenerationStep(prepareInput(), repositories, passingHandlers(), timestamp);
    assert.equal(prepared.status, 'prepared');
    let failAcknowledgement = true;
    const wrapped = {
      ...repositories,
      plans: {
        read: repositories.plans.read.bind(repositories.plans),
        apply: async (...args) => {
          const action = args[2];
          if (action.type === 'accept-step' && failAcknowledgement) {
            failAcknowledgement = false;
            throw new Error('simulated plan acknowledgement failure');
          }
          return repositories.plans.apply(...args);
        }
      }
    };
    await assert.rejects(
      () => commitGenerationStep({ ...prepareInput(), expectedPlanRevision: 6 }, wrapped, timestamp),
      /acknowledgement failure/
    );
    assert.equal((await repositories.scenes.read(scope.documentId)).revision, 2);
    assert.equal((await repositories.plans.read(scope)).revision, 6);
    const recovered = await commitGenerationStep({ ...prepareInput(), expectedPlanRevision: 6 }, wrapped, timestamp);
    assert.equal(recovered.status, 'committed');
    assert.equal(recovered.recovered, true);
    assert.equal(recovered.plan.pageRuns[0].steps[0].status, 'accepted');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('candidate transaction cannot edit an unrelated sibling outside the planned target', async () => {
  const { root, repositories } = await environment();
  try {
    const handlers = {
      generate() {
        return {
          transactionId: 'candidate-outside-scope', baseRevision: 1, author: 'ai',
          operations: [{ op: 'update-node', nodeId: 'group-hero-copy', patches: [{ path: ['frame', 'x'], value: 99 }] }]
        };
      },
      verify() { throw new Error('must not verify'); }
    };
    const result = await prepareGenerationStep(prepareInput(), repositories, handlers, timestamp);
    assert.equal(result.status, 'failed');
    assert.match(result.error.message, /outside the current step scope/);
    assert.equal((await repositories.scenes.read(scope.documentId)).revision, 1);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
