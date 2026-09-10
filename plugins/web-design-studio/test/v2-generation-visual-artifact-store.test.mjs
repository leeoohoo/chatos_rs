import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { GenerationVisualArtifactStore } from '../dist/v2-generation-visual-artifact-store.test.mjs';

function source(scope, artifactId = 'snapshot:one') {
  const createdAt = '2026-09-08T18:00:00.000Z';
  return {
    schemaVersion: 1,
    artifact: { artifactId, kind: 'page-snapshot', revision: 4, viewportWidth: 1440, createdAt },
    scope,
    pageId: 'page-home',
    rootNodeId: 'root-home',
    width: 1440,
    height: 1200,
    grounding: [{ nodeId: 'hero-heading', parentId: 'hero', pageId: 'page-home', rect: { x: 80, y: 120, width: 620, height: 96 }, depth: 2 }],
    createdAt
  };
}

test('visual artifact store atomically persists scoped PNGs and grounding metadata', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-visual-artifact-'));
  const scope = { projectId: 'project-visual', documentId: 'document-visual' };
  const image = Buffer.from('fake-png-for-storage-test');
  try {
    const first = new GenerationVisualArtifactStore(root);
    const created = await first.create(source(scope), image);
    assert.equal(created.mimeType, 'image/png');
    assert.match(created.imageFileName, /^visual-artifact-[a-f0-9]{64}\.png$/);

    const restarted = new GenerationVisualArtifactStore(root);
    const restored = await restarted.readImage(scope, 'snapshot:one');
    assert.deepEqual(restored.data, image);
    assert.equal(restored.record.grounding[0].nodeId, 'hero-heading');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('visual artifact identities are isolated by the host project and document scope', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-visual-scope-'));
  const scope = { projectId: 'project-a', documentId: 'document-a' };
  try {
    const store = new GenerationVisualArtifactStore(root);
    await store.create(source(scope), Buffer.from('scope-a'));
    await assert.rejects(
      () => store.read({ projectId: 'project-b', documentId: 'document-a' }, 'snapshot:one'),
      (error) => error?.code === 'ENOENT'
    );
    await assert.rejects(
      () => store.read({ projectId: 'project-a', documentId: 'document-b' }, 'snapshot:one'),
      (error) => error?.code === 'ENOENT'
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
