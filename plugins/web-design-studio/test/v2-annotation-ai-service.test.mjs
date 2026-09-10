import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { AnnotationAiService } from '../dist/v2-annotation-ai-service.test.mjs';
import { createBlankSceneDocument, createSceneNodeBase, indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { SceneDocumentStore } from '../dist/v2-scene-store.test.mjs';

test('annotation AI service binds a human note to one page, revision, stable node, and visual crop', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'annotation-ai-service-'));
  try {
    const scenes = new SceneDocumentStore(root);
    const document = createBlankSceneDocument('Annotation service');
    document.documentId = 'design-annotation';
    document.pages[0].id = 'page-pricing';
    document.pages[0].children = [{
      ...createSceneNodeBase('frame', 'Pricing root', { x: 0, y: 0, width: 1440, height: 900 }),
      id: 'frame-pricing',
      role: 'page-root',
      children: [{
        ...createSceneNodeBase('text', 'Price heading', { x: 80, y: 100, width: 520, height: 80 }),
        id: 'text-price',
        content: 'Simple pricing'
      }]
    }];
    indexSceneDocument(document).get('text-price').node.annotations.push({
      id: 'annotation:hierarchy', author: 'human', body: 'Make the price hierarchy clearer.', status: 'open', createdAt: new Date().toISOString()
    });
    const created = await scenes.create(document);
    const calls = [];
    const service = new AnnotationAiService({
      projectId: 'project-product',
      scenes,
      assertDocumentInScope: async (documentId) => assert.equal(documentId, created.documentId),
      visuals: {
        async captureRegion(input) {
          calls.push(input);
          return {
            capture: {
              artifact: { artifactId: 'region-crop:test', revision: created.revision, viewportWidth: input.viewportWidth },
              pageId: input.pageId, rootNodeId: 'frame-pricing', width: 552, height: 112, groundingCount: 1
            },
            artifacts: [], calibration: { passed: true }, diagnostics: [],
            __images: [{ label: 'region', data: 'cG5n', mimeType: 'image/png' }]
          };
        }
      }
    });
    const prepared = await service.prepare({
      documentId: created.documentId,
      nodeId: 'text-price',
      annotationId: 'annotation:hierarchy',
      viewportWidth: 1440
    });
    assert.equal(prepared.task.scope.projectId, 'project-product');
    assert.equal(prepared.task.pageId, 'page-pricing');
    assert.equal(prepared.task.targetNodeId, 'text-price');
    assert.equal(prepared.task.baseRevision, created.revision);
    assert.equal(prepared.visualContext.capture.artifact.artifactId, 'region-crop:test');
    assert.deepEqual(calls, [{ documentId: created.documentId, pageId: 'page-pricing', viewportWidth: 1440, nodeId: 'text-price', padding: 24 }]);
    assert.equal(prepared.nextAction.tool, 'web_design_edit_scene');
    assert.equal(prepared.__images.length, 1);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('annotation AI service rejects a screenshot from another Scene revision', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'annotation-ai-service-stale-'));
  try {
    const scenes = new SceneDocumentStore(root);
    const document = createBlankSceneDocument('Stale annotation service');
    document.documentId = 'design-stale';
    document.pages[0].id = 'page-home';
    document.pages[0].children = [{
      ...createSceneNodeBase('text', 'Heading', { x: 0, y: 0, width: 300, height: 80 }),
      id: 'text-heading', content: 'Heading',
      annotations: [{ id: 'annotation:stale', author: 'human', body: 'Review', status: 'open', createdAt: new Date().toISOString() }]
    }];
    const created = await scenes.create(document);
    const service = new AnnotationAiService({
      projectId: 'project-product', scenes, assertDocumentInScope: async () => undefined,
      visuals: { async captureRegion() {
        return {
          capture: { artifact: { artifactId: 'region-crop:stale', revision: created.revision + 1 }, pageId: 'page-home', rootNodeId: 'text-heading', width: 300, height: 80, groundingCount: 1 },
          __images: []
        };
      } }
    });
    await assert.rejects(() => service.prepare({
      documentId: created.documentId, nodeId: 'text-heading', annotationId: 'annotation:stale', viewportWidth: 1440
    }), /another Scene revision|changed/i);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
