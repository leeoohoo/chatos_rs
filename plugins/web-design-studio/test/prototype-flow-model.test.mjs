import assert from 'node:assert/strict';
import test from 'node:test';
import { buildPrototypeFlowConnections, prototypeFlowPath, scenePrototypeFlowSources } from '../dist/prototype-flow-model.test.mjs';
import { createBlankSceneDocument, createSceneNodeBase } from '../dist/v2-scene-schema.test.mjs';

test('prototype flow connections follow component and artboard world coordinates', () => {
  const [connection] = buildPrototypeFlowConnections([{
    componentId: 'cta', pageId: 'home', targetPageId: 'signup-modal', x: 100, y: 200, width: 160, height: 48
  }], [
    { artboardId: 'home-board', pageId: 'home', surfaceKind: 'page', viewportWidth: 1200, viewportHeight: 900, x: 400, y: 80 },
    { artboardId: 'modal-board', pageId: 'signup-modal', surfaceKind: 'modal', viewportWidth: 720, viewportHeight: 720, x: 1200, y: 160 }
  ]);
  assert.deepEqual(connection.start, { x: 660, y: 304 });
  assert.deepEqual(connection.end, { x: 1200, y: 275.2 });
  assert.equal(connection.targetSurfaceKind, 'modal');
  assert.match(prototypeFlowPath(connection), /^M 660 304 C /);
});

test('prototype flow ignores missing and same-artboard targets', () => {
  const artboards = [{ artboardId: 'home-board', pageId: 'home', surfaceKind: 'page', viewportWidth: 1200, viewportHeight: 900, x: 0, y: 0 }];
  assert.deepEqual(buildPrototypeFlowConnections([
    { componentId: 'same', pageId: 'home', targetPageId: 'home', x: 0, y: 0, width: 100, height: 40 },
    { componentId: 'missing', pageId: 'home', targetPageId: 'missing', x: 0, y: 0, width: 100, height: 40 }
  ], artboards), []);
});

test('prototype flow derives sources from responsive Scene layout instead of legacy components', () => {
  const scene = createBlankSceneDocument('Prototype flow');
  scene.pages[0].id = 'home';
  scene.pages[0].children = [{
    ...createSceneNodeBase('frame', 'Root', { x: 0, y: 0, width: 1200, height: 900 }),
    id: 'root-home',
    children: [{
      ...createSceneNodeBase('shape', 'Open dialog', { x: 80, y: 120, width: 180, height: 52 }),
      id: 'cta-dialog',
      shape: 'rectangle',
      prototypeLink: { trigger: 'click', action: 'overlay', targetPageId: 'dialog' }
    }]
  }];
  scene.pages.push({ id: 'dialog', name: 'Dialog', children: [] });
  scene.responsiveRules = [{
    id: 'mobile-flow', name: 'Mobile flow', maxWidth: 500, variableModes: {},
    nodeOverrides: [{ nodeId: 'cta-dialog', visible: false }]
  }];
  const artboards = [
    { artboardId: 'home-board', pageId: 'home', surfaceKind: 'page', viewportWidth: 1200, viewportHeight: 900, x: 20, y: 30 },
    { artboardId: 'dialog-board', pageId: 'dialog', surfaceKind: 'modal', viewportWidth: 720, viewportHeight: 640, x: 1500, y: 100 }
  ];
  assert.deepEqual(scenePrototypeFlowSources(scene, artboards), [{
    componentId: 'cta-dialog', pageId: 'home', targetPageId: 'dialog', x: 80, y: 120, width: 180, height: 52
  }]);
  assert.deepEqual(scenePrototypeFlowSources(scene, [{ ...artboards[0], viewportWidth: 390 }, artboards[1]]), []);
});
