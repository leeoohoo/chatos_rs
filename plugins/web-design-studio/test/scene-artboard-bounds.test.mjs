import assert from 'node:assert/strict';
import test from 'node:test';
import { sceneArtboardContentBounds, sceneArtboardContentHeight } from '../dist/scene-artboard-bounds.test.mjs';
import { createBlankSceneDocument, createSceneNodeBase } from '../dist/v2-scene-schema.test.mjs';

function absoluteShape(id, y, height = 40) {
  const node = {
    ...createSceneNodeBase('shape', id, { x: 120, y, width: 420, height }),
    id,
    type: 'shape',
    shape: 'rectangle'
  };
  node.layout.position = 'absolute';
  node.layout.sizingX = 'fixed';
  node.layout.sizingY = 'fixed';
  return node;
}

function sceneWithButton(buttonY) {
  const scene = createBlankSceneDocument('Auto height artboard');
  scene.documentId = 'scene-auto-height';
  scene.pages[0].id = 'page-login';
  const root = {
    ...createSceneNodeBase('frame', 'Login root', { x: 0, y: 0, width: 1440, height: 768 }),
    id: 'root-login',
    type: 'frame',
    role: 'page-root',
    children: [absoluteShape('button-dropped', buttonY)]
  };
  root.layout.mode = 'free';
  root.layout.sizingX = 'fixed';
  root.layout.sizingY = 'fixed';
  scene.pages[0].children = [root];
  return scene;
}

test('an absolute component below the old root grows the artboard to its real bottom edge', () => {
  const scene = sceneWithButton(1040);
  const bounds = sceneArtboardContentBounds(scene, 'page-login', 1440, 900);
  assert.equal(bounds.height, 1080);
  assert.equal(bounds.nodeCount, 2);
});

test('stored viewport height remains the minimum while populated artboards can grow', () => {
  const scene = sceneWithButton(700);
  assert.equal(sceneArtboardContentHeight(scene, 'page-login', 1440, 900), 900);
});

test('measuring one page ignores content on every other artboard', () => {
  const scene = sceneWithButton(700);
  const otherRoot = {
    ...createSceneNodeBase('frame', 'Other root', { x: 0, y: 0, width: 800, height: 500 }),
    id: 'root-other',
    type: 'frame',
    role: 'page-root',
    children: [absoluteShape('other-bottom-node', 1800, 100)]
  };
  otherRoot.layout.mode = 'free';
  scene.pages.push({ id: 'page-other', name: 'Other', children: [otherRoot] });
  assert.equal(sceneArtboardContentHeight(scene, 'page-login', 1440, 1), 768);
  assert.equal(sceneArtboardContentHeight(scene, 'page-other', 800, 1), 1900);
});
