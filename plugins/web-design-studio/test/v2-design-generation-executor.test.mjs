import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { createDesignGenerationPlan } from '../dist/v2-design-protocol.test.mjs';
import { executeDesignGenerationPlan } from '../dist/v2-design-generation-executor.test.mjs';
import { createSceneNodeBase } from '../dist/v2-scene-schema.test.mjs';
import { SceneDocumentStore } from '../dist/v2-scene-store.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

function protocol() {
  const scope = { projectId: 'project-generation', documentId: 'scene-test' };
  const brief = {
    schemaVersion: 1, briefId: 'brief-generation', scope,
    objective: 'Generate a product page.', audience: ['Design teams'],
    brand: { name: 'Arc', attributes: ['precise', 'calm'], visualReferences: [] },
    content: { locale: 'zh-CN', pages: [{ pageKey: 'home', name: 'Home', purpose: 'Explain the product', sections: [{ sectionKey: 'proof', role: 'social-proof', goal: 'Build trust', contentRequirements: ['evidence'] }] }] },
    constraints: { viewportWidths: [390, 1440], accessibilityLevel: 'AA', forbiddenPatterns: ['dashboard layout'] },
    createdAt: '2026-09-08T02:00:00.000Z'
  };
  const spec = {
    schemaVersion: 1, specId: 'spec-generation', briefId: brief.briefId, scope, sourceRevision: 1,
    designPrinciples: ['Evidence first', 'Calm visual rhythm'],
    tokenIntents: [{ tokenKey: 'color-brand', type: 'color', purpose: 'Brand accent', modes: ['light'] }],
    pages: [{ pageKey: 'home', sections: [{
      sectionKey: 'proof', nodeRole: 'social-proof', visualPriority: 'secondary',
      layoutIntent: { mode: 'grid', minColumnWidth: 220, widthBehavior: 'bounded', maxContentWidth: 1200 },
      componentStrategy: { source: 'native', preferredLibraries: [], requiredInteractions: [] },
      responsiveIntent: ['Collapse continuously']
    }] }],
    globalResponsiveIntent: ['Full-width background'], createdAt: '2026-09-08T02:01:00.000Z'
  };
  return { brief, spec, plan: createDesignGenerationPlan(brief, spec) };
}

function sectionNode() {
  const base = createSceneNodeBase('frame', 'Proof', { x: 0, y: 0, width: 800, height: 200 }, 'ai');
  return { ...base, type: 'frame', id: 'frame-generated-proof', role: 'social-proof', children: [] };
}

test('generation executor commits design system and semantic sections with monotonic revisions and diffs', async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'scene-generation-'));
  try {
    const store = new SceneDocumentStore(directory);
    await store.create(nestedWebsite());
    const { plan } = protocol();
    const seenScopes = [];
    const run = await executeDesignGenerationPlan(plan, store, {
      createTransaction(step, document, scope) {
        seenScopes.push(scope);
        if (step.kind === 'create-design-system') return {
          transactionId: 'generation-design-system', baseRevision: document.revision, author: 'ai',
          operations: [{ op: 'insert-variable-collection', index: 0, collection: {
            id: 'variables-generation', name: 'Generation', modes: [{ id: 'mode-light', name: 'Light' }],
            variables: [{ id: 'variable-generation-brand', name: 'Brand', type: 'color', valuesByMode: { 'mode-light': '#3457ff' } }]
          } }]
        };
        return { transactionId: 'generation-proof', baseRevision: document.revision, author: 'ai', operations: [{ op: 'insert-node', parentId: 'frame-desktop', index: 1, node: sectionNode() }] };
      },
      solveLayout(step) { return step.viewportWidths.map((viewportWidth) => ({ viewportWidth, rootNodeId: 'frame-desktop', errorDiagnostics: 0, warningDiagnostics: 0 })); },
      renderSnapshots(step) { return step.viewportWidths.map((viewportWidth) => ({ viewportWidth, snapshotId: `snapshot-${viewportWidth}` })); },
      critique() { return { verdict: 'pass', issueIds: [], summary: 'The page passes the initial visual review.' }; }
    });
    assert.equal(run.status, 'completed');
    assert.equal(run.baseRevision, 1);
    assert.equal(run.finalRevision, 3);
    assert.deepEqual(seenScopes, [plan.scope, plan.scope]);
    assert.equal(run.steps[0].diff.summary.entitiesAdded, 2);
    assert.ok(run.steps[1].diff.changes.some((change) => change.kind === 'entity-added' && change.entityId === 'frame-generated-proof'));
    assert.equal(run.steps.at(-1).critique.verdict, 'pass');
    const stored = await store.read('scene-test');
    assert.equal(stored.variableCollections[0].id, 'variables-generation');
    assert.equal(stored.pages[0].children[0].children[0].children.at(-1).id, 'frame-generated-proof');
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test('generation executor stops at a failed atomic transaction and blocks all dependent visual steps', async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'scene-generation-failure-'));
  try {
    const store = new SceneDocumentStore(directory);
    await store.create(nestedWebsite());
    const { plan } = protocol();
    const run = await executeDesignGenerationPlan(plan, store, {
      createTransaction(step, document) {
        if (step.kind === 'create-design-system') return {
          transactionId: 'generation-design-system', baseRevision: document.revision, author: 'ai',
          operations: [{ op: 'insert-variable-collection', index: 0, collection: { id: 'variables-invalid', name: 'Invalid', modes: [], variables: [] } }]
        };
        throw new Error('must not execute');
      },
      solveLayout() { throw new Error('must not execute'); },
      renderSnapshots() { throw new Error('must not execute'); },
      critique() { throw new Error('must not execute'); }
    });
    assert.equal(run.status, 'failed');
    assert.equal(run.finalRevision, 1);
    assert.equal(run.steps[0].status, 'failed');
    assert.match(run.steps[0].error, /modes are invalid/);
    assert.ok(run.steps.slice(1).every((step) => step.status === 'blocked'));
    assert.equal((await store.read('scene-test')).variableCollections.length, 1);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test('generation executor rejects stale plans, wrong scopes, and non-AI mutation transactions', async () => {
  const document = nestedWebsite();
  document.revision = 1;
  const { plan } = protocol();
  const repository = {
    async read() { return structuredClone(document); },
    async apply() { throw new Error('must not apply'); }
  };
  const handlers = {
    createTransaction(_step, scene) { return { transactionId: 'human-change', baseRevision: scene.revision, author: 'human', operations: [{ op: 'rename-page', pageId: 'page-home', name: 'Changed' }] }; },
    solveLayout() { return []; }, renderSnapshots() { return []; }, critique() { return { verdict: 'pass', issueIds: [], summary: 'Pass' }; }
  };
  const nonAi = await executeDesignGenerationPlan(plan, repository, handlers);
  assert.equal(nonAi.status, 'failed');
  assert.match(nonAi.steps[0].error, /must be authored by AI/);
  const stale = structuredClone(plan);
  stale.baseRevision = 99;
  await assert.rejects(() => executeDesignGenerationPlan(stale, repository, handlers), /revision conflict/);
  const wrongDocument = structuredClone(plan);
  wrongDocument.scope.documentId = 'scene-other';
  await assert.rejects(() => executeDesignGenerationPlan(wrongDocument, repository, handlers), /documentId does not match/);
});
