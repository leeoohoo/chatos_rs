import assert from 'node:assert/strict';
import { mkdtemp, readdir, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { GenerationPlanStore, GenerationPlanRevisionConflictError } from '../dist/v2-generation-plan-store.test.mjs';
import { generationPlanFixture } from './helpers/v2-generation-plan-fixture.mjs';

test('generation plan store survives restart with exact project and document scope', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-generation-plan-'));
  const scope = { projectId: 'project-host-scope', documentId: 'website-ai-site' };
  try {
    const firstStore = new GenerationPlanStore(root);
    const created = await firstStore.create(generationPlanFixture());
    assert.equal(created.revision, 1);
    const ready = await firstStore.apply(scope, 1, { type: 'mark-ready' }, '2026-09-08T10:00:00.000Z');
    const running = await firstStore.apply(scope, 2, { type: 'start-page', pageId: 'home' }, '2026-09-08T10:01:00.000Z');
    assert.equal(running.plan.activePageId, 'home');

    const restartedStore = new GenerationPlanStore(root);
    const restored = await restartedStore.read(scope);
    assert.equal(restored.revision, 3);
    assert.equal(restored.status, 'running');
    assert.equal(restored.activePageId, 'home');
    assert.equal(ready.replayed, false);
    const files = await readdir(root);
    assert.equal(files.filter((file) => file.endsWith('.json')).length, 1);
    assert.equal(files.some((file) => file.endsWith('.tmp')), false);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('idempotent step replay does not increment plan revision or duplicate attempts', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-generation-idempotent-'));
  const scope = { projectId: 'project-host-scope', documentId: 'website-ai-site' };
  try {
    const store = new GenerationPlanStore(root);
    await store.create(generationPlanFixture());
    await store.apply(scope, 1, { type: 'mark-ready' });
    await store.apply(scope, 2, { type: 'start-page', pageId: 'home' });
    const action = { type: 'start-step', pageId: 'home', stepId: 'home-structure', attemptId: 'store-attempt-1', idempotencyKey: 'store-key-1', baseRevision: 7 };
    const started = await store.apply(scope, 3, action);
    assert.equal(started.plan.revision, 4);
    const replayed = await store.apply(scope, 4, action);
    assert.equal(replayed.replayed, true);
    assert.equal(replayed.plan.revision, 4);
    assert.equal(replayed.plan.pageRuns[0].steps[0].attempts.length, 1);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('generation plan store rejects stale concurrent writers', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-generation-concurrency-'));
  const scope = { projectId: 'project-host-scope', documentId: 'website-ai-site' };
  try {
    const store = new GenerationPlanStore(root);
    await store.create(generationPlanFixture());
    const attempts = await Promise.allSettled([
      store.apply(scope, 1, { type: 'mark-ready' }),
      store.apply(scope, 1, { type: 'mark-ready' })
    ]);
    assert.equal(attempts.filter((attempt) => attempt.status === 'fulfilled').length, 1);
    const rejected = attempts.find((attempt) => attempt.status === 'rejected');
    assert.ok(rejected.reason instanceof GenerationPlanRevisionConflictError);
    assert.equal(rejected.reason.actualRevision, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('project scope participates in physical plan isolation', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-generation-scope-'));
  try {
    const store = new GenerationPlanStore(root);
    await store.create(generationPlanFixture());
    await assert.rejects(
      () => store.read({ projectId: 'project-forged', documentId: 'website-ai-site' }),
      (error) => error?.code === 'ENOENT'
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
