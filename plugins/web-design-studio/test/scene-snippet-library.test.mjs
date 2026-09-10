import assert from 'node:assert/strict';
import test from 'node:test';
import { createSceneSnippet, instantiateSceneSnippet, parseSceneSnippets } from '../dist/scene-snippet-library.test.mjs';
import { createBlankSceneDocument, createSceneNodeBase } from '../dist/v2-scene-schema.test.mjs';

function fixture() {
  const scene = createBlankSceneDocument('Snippet fixture');
  scene.documentId = 'snippet-fixture';
  scene.pages[0].id = 'page-home';
  const root = createSceneNodeBase('frame', 'Root', { x: 0, y: 0, width: 1200, height: 900 });
  root.role = 'page-root';
  root.children = [
    Object.assign(createSceneNodeBase('shape', 'One', { x: 100, y: 80, width: 120, height: 60 }), { shape: 'rectangle' }),
    Object.assign(createSceneNodeBase('shape', 'Two', { x: 250, y: 120, width: 80, height: 90 }), { shape: 'ellipse' })
  ];
  scene.pages[0].children = [root];
  return scene;
}

test('Scene snippets preserve a visual group and instantiate it with fresh ids', () => {
  const scene = fixture();
  const ids = scene.pages[0].children[0].children.map((node) => node.id);
  const snippet = createSceneSnippet(scene, ids, 'Feature pair');
  assert.equal(snippet.width, 230);
  assert.equal(snippet.height, 130);
  const instance = instantiateSceneSnippet(snippet, 40, 50);
  assert.equal(instance.type, 'group');
  assert.deepEqual(instance.frame, { x: 40, y: 50, width: 230, height: 130 });
  assert.equal(instance.children.length, 2);
  assert.notEqual(instance.children[0].id, ids[0]);
  assert.deepEqual(instance.children.map((node) => [node.frame.x, node.frame.y]), [[0, 0], [150, 40]]);
});

test('Scene snippets reject roots from unrelated containers and parse storage defensively', () => {
  const scene = fixture();
  const secondRoot = createSceneNodeBase('frame', 'Other', { x: 0, y: 0, width: 400, height: 300 });
  secondRoot.children = [Object.assign(createSceneNodeBase('shape', 'Other shape', { x: 10, y: 10, width: 20, height: 20 }), { shape: 'rectangle' })];
  scene.pages[0].children.push(secondRoot);
  assert.throws(() => createSceneSnippet(scene, [scene.pages[0].children[0].children[0].id, secondRoot.children[0].id], 'Bad'));
  assert.deepEqual(parseSceneSnippets('{'), []);
});
