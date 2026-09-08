import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  V2_BASELINE_VIEWPORTS,
  V2_PRESERVED_CAPABILITIES,
  V2_QUALITY_RUBRIC,
  V2_WEBSITE_BENCHMARKS,
  validatePhase0Baseline
} from '../dist/v2-phase0-baseline.test.mjs';
import { WebDesignDocumentStore, RevisionConflictError } from '../dist/document-store.test.mjs';
import { runtimeScopeFingerprint } from '../dist/runtime-scope.test.mjs';

test('v2 phase 0 defines twelve materially different website benchmarks', () => {
  assert.deepEqual(validatePhase0Baseline(), []);
  assert.equal(V2_WEBSITE_BENCHMARKS.length, 12);
  assert.equal(new Set(V2_WEBSITE_BENCHMARKS.map((benchmark) => benchmark.category)).size, 12);
  assert.ok(V2_WEBSITE_BENCHMARKS.every((benchmark) => benchmark.forbiddenPatterns.length >= 3));
});

test('v2 phase 0 covers continuous responsive widths through 8K', () => {
  assert.deepEqual(V2_BASELINE_VIEWPORTS.map((viewport) => viewport.width), [320, 390, 768, 1024, 1280, 1440, 1920, 2560, 3840, 7680]);
  assert.equal(V2_QUALITY_RUBRIC.reduce((sum, criterion) => sum + criterion.weight, 0), 100);
  assert.ok(V2_QUALITY_RUBRIC.some((criterion) => criterion.id === 'editability'));
});

test('v2 freezes project scope, exact persistence, and revision safety as retained capabilities', async () => {
  assert.equal(V2_PRESERVED_CAPABILITIES.length, 6);
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-v2-baseline-'));
  const store = new WebDesignDocumentStore(root);
  const environmentNames = ['CHATOS_CONTEXT_SCOPE', 'CHATOS_PROJECT_ID', 'CHATOS_WORKSPACE_ID', 'CHATOS_USER_ID'];
  const originalEnvironment = Object.fromEntries(environmentNames.map((name) => [name, process.env[name]]));
  try {
    process.env.CHATOS_CONTEXT_SCOPE = 'project';
    process.env.CHATOS_PROJECT_ID = 'phase0-project-a';
    process.env.CHATOS_WORKSPACE_ID = 'phase0-workspace';
    process.env.CHATOS_USER_ID = 'phase0-user';
    const firstScope = runtimeScopeFingerprint(root);
    process.env.CHATOS_PROJECT_ID = 'phase0-project-b';
    assert.notEqual(runtimeScopeFingerprint(root), firstScope);

    const project = await store.createProject('Phase 0 baseline');
    const document = await store.createInProject(project.projectId, 'Pixel-stable design', false);
    document.components[0].x = 237.125;
    document.components[0].y = 418.875;
    document.components[0].width = 903.625;
    const saved = await store.replace(document, document.revision);
    const filePath = path.join(root, `${saved.documentId}.web-design.json`);
    const fileBeforeRead = await readFile(filePath, 'utf8');
    const reopened = await store.read(saved.documentId);
    const fileAfterRead = await readFile(filePath, 'utf8');
    assert.equal(reopened.components[0].x, 237.125);
    assert.equal(reopened.components[0].y, 418.875);
    assert.equal(reopened.components[0].width, 903.625);
    assert.equal(fileAfterRead, fileBeforeRead);

    await assert.rejects(
      () => store.replace({ ...reopened, title: 'stale overwrite' }, document.revision),
      (error) => error instanceof RevisionConflictError && error.actualRevision === saved.revision
    );
  } finally {
    for (const name of environmentNames) {
      if (originalEnvironment[name] === undefined) delete process.env[name];
      else process.env[name] = originalEnvironment[name];
    }
    await rm(root, { recursive: true, force: true });
  }
});
