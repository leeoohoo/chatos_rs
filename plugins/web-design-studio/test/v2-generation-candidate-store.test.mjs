import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { GenerationCandidateStore } from '../dist/v2-generation-candidate-store.test.mjs';

function candidate() {
  return {
    schemaVersion: 1,
    candidateId: 'candidate-attempt-one',
    planId: 'plan-candidate',
    scope: { projectId: 'project-candidate', documentId: 'scene-test' },
    pageId: 'page-home',
    stepId: 'home-visual',
    attemptId: 'attempt-one',
    idempotencyKey: 'home-visual-base-1',
    baseRevision: 1,
    transaction: {
      transactionId: 'candidate-transaction-one', baseRevision: 1, author: 'ai',
      operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['appearance', 'typography', 'fontSize'], value: 72 }] }]
    },
    artifacts: [
      { artifactId: 'snapshot-one', kind: 'page-snapshot', revision: 2, viewportWidth: 1440, createdAt: '2026-09-08T11:10:00.000Z' },
      { artifactId: 'quality-one', kind: 'quality-report', revision: 2, createdAt: '2026-09-08T11:10:00.000Z' }
    ],
    protectionConflicts: [],
    qualitySummary: 'Visual hierarchy passes.',
    issueIds: [],
    createdAt: '2026-09-08T11:10:00.000Z',
    updatedAt: '2026-09-08T11:10:00.000Z'
  };
}

test('candidate store persists a verified transaction independently from the Scene', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-candidate-store-'));
  try {
    const first = new GenerationCandidateStore(root);
    await first.create(candidate());
    const restarted = new GenerationCandidateStore(root);
    const restored = await restarted.read(candidate().scope, 'attempt-one');
    assert.equal(restored.transaction.transactionId, 'candidate-transaction-one');
    assert.equal(restored.artifacts[0].kind, 'page-snapshot');
    assert.equal(restored.baseRevision, 1);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('candidate creation is idempotent only for the same key and exact transaction', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-candidate-idempotent-'));
  try {
    const store = new GenerationCandidateStore(root);
    const first = await store.create(candidate());
    const replay = await store.create(candidate());
    assert.deepEqual(replay, first);
    const conflicting = candidate();
    conflicting.transaction.operations[0].patches[0].value = 96;
    await assert.rejects(() => store.create(conflicting), /already exists/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
