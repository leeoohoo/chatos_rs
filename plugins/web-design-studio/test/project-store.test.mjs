import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { WebDesignDocumentStore } from '../dist/document-store.test.mjs';
import { createLandingPage } from '../dist/templates.test.mjs';

test('projects own design membership without changing design layout data', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-project-store-'));
  const store = new WebDesignDocumentStore(root);
  try {
    const source = await store.createProject('产品官网', '官网的多个设计方向');
    const target = await store.createProject('活动专题');
    const blank = await store.createInProject(source.projectId, '空白方案', true);
    const landing = await store.createInProject(source.projectId, '落地页方案', false);

    assert.equal(blank.components.length, 0);
    assert.ok(landing.components.length > 0);
    assert.deepEqual((await store.readProject(source.projectId)).designIds, [blank.documentId, landing.documentId]);
    assert.deepEqual((await store.listInProject(source.projectId)).map((item) => item.documentId).sort(), [blank.documentId, landing.documentId].sort());

    await store.moveDocument(blank.documentId, target.projectId, source.projectId);
    assert.deepEqual((await store.readProject(source.projectId)).designIds, [landing.documentId]);
    assert.deepEqual((await store.readProject(target.projectId)).designIds, [blank.documentId]);

    await store.remove(landing.documentId);
    assert.deepEqual((await store.readProject(source.projectId)).designIds, []);
    await store.deleteProject(source.projectId, false);
    assert.equal((await store.read(blank.documentId)).title, '空白方案');
    await store.deleteProject(target.projectId, true);
    await assert.rejects(() => store.read(blank.documentId), /ENOENT/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('legacy migration creates only a project index and preserves the design file byte for byte', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-legacy-project-'));
  const store = new WebDesignDocumentStore(root);
  try {
    const legacy = createLandingPage('升级前的网站');
    legacy.components[0].x = 912.25;
    legacy.components[0].y = 1480.75;
    legacy.components[0].width = 486.5;
    legacy.components[0].responsive = {
      ...legacy.components[0].responsive,
      mobile: { ...legacy.components[0].responsive?.mobile, x: 19.5, y: 811.25, width: 351.5 }
    };
    const saved = await store.writeNew(legacy);
    const file = path.join(root, `${saved.documentId}.web-design.json`);
    const before = await readFile(file, 'utf8');

    const migrated = await store.ensureLegacyProject();
    const after = await readFile(file, 'utf8');

    assert.ok(migrated);
    assert.deepEqual(migrated.designIds, [saved.documentId]);
    assert.equal(after, before);
    assert.equal(await store.ensureLegacyProject(), undefined);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
