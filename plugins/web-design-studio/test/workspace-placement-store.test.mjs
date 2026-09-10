import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { WorkspacePlacementStore } from '../dist/workspace-placement-store.test.mjs';

test('workspace placement persists camera separately from design data and isolates runtime scopes', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-workspace-placement-'));
  try {
    const store = new WorkspacePlacementStore(root);
    const created = await store.readOrCreate('scope-a', 'document-a');
    assert.equal(created.schemaVersion, 2);
    assert.deepEqual(created.camera, { x: 0, y: 0, zoom: 1 });
    const updated = await store.updateCamera('scope-a', 'document-a', { x: -4200, y: 7300, zoom: 2.5 });
    assert.equal(updated.revision, 2);
    const withArtboards = await store.updateArtboards('scope-a', 'document-a', [
      { artboardId: 'home-page', pageId: 'home', surfaceKind: 'page', viewportWidth: 1440, viewportHeight: 900, x: 0, y: 0 },
      { artboardId: 'sign-in-modal', pageId: 'sign-in', surfaceKind: 'modal', viewportWidth: 720, viewportHeight: 720, x: 1600, y: 0 }
    ]);
    assert.equal(withArtboards.revision, 3);
    assert.equal(withArtboards.artboards.length, 2);
    assert.deepEqual(withArtboards.camera, updated.camera);
    assert.deepEqual((await new WorkspacePlacementStore(root).readOrCreate('scope-a', 'document-a')).camera, { x: -4200, y: 7300, zoom: 2.5 });
    assert.deepEqual((await store.readOrCreate('scope-b', 'document-a')).camera, { x: 0, y: 0, zoom: 1 });
    assert.deepEqual((await new WorkspacePlacementStore(root).readOrCreate('scope-a', 'document-a')).artboards, withArtboards.artboards);
    await assert.rejects(() => store.updateArtboards('scope-a', 'document-a', [
      { projectionId: 'legacy-mobile', pageId: 'home', viewportWidth: 390, viewportHeight: 844, x: 0, y: 0 }
    ]), /artboardId is invalid/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
