import assert from 'node:assert/strict';
import test from 'node:test';
import { createBlankSceneDocument, createSceneNodeBase, indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { applySceneTransaction } from '../dist/v2-scene-transaction.test.mjs';
import {
  createAlignSceneNodesTransaction,
  createAutoLayoutSceneFrameTransaction,
  createDistributeSceneNodesTransaction,
  createMoveSceneNodesTransaction,
  createReorderSceneNodesTransaction,
  createResizeSceneNodeTransaction,
  createUngroupSceneNodeTransaction,
  createWrapSceneNodesTransaction,
  resizeSceneRect
} from '../dist/v2-scene-editor-transaction.test.mjs';
import { solveSceneLayout } from '../dist/v2-layout-engine.test.mjs';

function freeScene() {
  const document = createBlankSceneDocument('Editor transaction');
  document.documentId = 'scene-editor-transaction';
  document.pages[0].id = 'page-home';
  const first = { ...createSceneNodeBase('shape', 'First', { x: 100, y: 80, width: 120, height: 60 }), id: 'shape-first', shape: 'rectangle' };
  const second = { ...createSceneNodeBase('shape', 'Second', { x: 280, y: 150, width: 90, height: 110 }), id: 'shape-second', shape: 'ellipse' };
  const third = { ...createSceneNodeBase('text', 'Third', { x: 420, y: 40, width: 100, height: 40 }), id: 'text-third', content: 'Third' };
  const root = {
    ...createSceneNodeBase('frame', 'Root', { x: 0, y: 0, width: 800, height: 600 }),
    id: 'frame-root',
    children: [first, second, third]
  };
  document.pages[0].children = [root];
  return document;
}

function positions(document, ids) {
  const solved = solveSceneLayout(document, { rootNodeId: 'frame-root', viewportWidth: 800, viewportHeight: 600 });
  return Object.fromEntries(ids.map((id) => {
    const box = solved.boxes.get(id);
    return [id, { x: box.x, y: box.y, width: box.width, height: box.height }];
  }));
}

test('Scene v2 group transaction preserves visual geometry and source order', () => {
  const source = freeScene();
  const before = positions(source, ['shape-first', 'shape-second', 'text-third']);
  const transaction = createWrapSceneNodesTransaction(source, {
    transactionId: 'wrap-group', author: 'human', nodeIds: ['shape-second', 'shape-first'],
    wrapperId: 'group-selection', kind: 'group', name: 'Selection'
  });
  const wrapped = applySceneTransaction(source, transaction).document;
  assert.deepEqual(positions(wrapped, ['shape-first', 'shape-second', 'text-third']), before);
  const root = indexSceneDocument(wrapped).get('frame-root').node;
  assert.deepEqual(root.children.map((node) => node.id), ['group-selection', 'text-third']);
  const group = indexSceneDocument(wrapped).get('group-selection').node;
  assert.deepEqual(group.frame, { x: 100, y: 80, width: 270, height: 180 });
  assert.deepEqual(group.children.map((node) => node.id), ['shape-first', 'shape-second']);
  assert.deepEqual(group.children.map((node) => ({ x: node.frame.x, y: node.frame.y })), [{ x: 0, y: 0 }, { x: 180, y: 70 }]);
  assert.equal(source.revision, 0);
});

test('Scene v2 frame transaction owns padding without moving selected nodes', () => {
  const source = freeScene();
  const before = positions(source, ['shape-first', 'shape-second']);
  const wrapped = applySceneTransaction(source, createWrapSceneNodesTransaction(source, {
    transactionId: 'wrap-frame', author: 'human', nodeIds: ['shape-first', 'shape-second'],
    wrapperId: 'frame-selection', kind: 'frame', name: 'Feature frame', padding: 16
  })).document;
  assert.deepEqual(positions(wrapped, ['shape-first', 'shape-second']), before);
  const frame = indexSceneDocument(wrapped).get('frame-selection').node;
  assert.deepEqual(frame.frame, { x: 84, y: 64, width: 302, height: 212 });
  assert.deepEqual(frame.layout.padding, { top: 16, right: 16, bottom: 16, left: 16 });
});

test('Scene v2 ungroup transaction restores siblings and keeps their visual positions', () => {
  const source = freeScene();
  const wrapped = applySceneTransaction(source, createWrapSceneNodesTransaction(source, {
    transactionId: 'wrap-roundtrip', author: 'human', nodeIds: ['shape-first', 'shape-second'],
    wrapperId: 'frame-roundtrip', kind: 'frame', name: 'Roundtrip', padding: 20
  })).document;
  const beforeUngroup = positions(wrapped, ['shape-first', 'shape-second', 'text-third']);
  const ungrouped = applySceneTransaction(wrapped, createUngroupSceneNodeTransaction(wrapped, {
    transactionId: 'ungroup-roundtrip', author: 'human', wrapperId: 'frame-roundtrip'
  })).document;
  assert.deepEqual(positions(ungrouped, ['shape-first', 'shape-second', 'text-third']), beforeUngroup);
  assert.deepEqual(indexSceneDocument(ungrouped).get('frame-root').node.children.map((node) => node.id), ['shape-first', 'shape-second', 'text-third']);
  assert.equal(indexSceneDocument(ungrouped).has('frame-roundtrip'), false);
});

test('Scene v2 wrapping rejects cross-parent and auto-layout selections', () => {
  const crossParent = freeScene();
  const root = indexSceneDocument(crossParent).get('frame-root').node;
  const first = root.children.shift();
  root.children.unshift({
    ...createSceneNodeBase('group', 'Nested', { x: 100, y: 80, width: 120, height: 60 }),
    id: 'group-nested',
    children: [{ ...first, frame: { ...first.frame, x: 0, y: 0 } }]
  });
  assert.throws(() => createWrapSceneNodesTransaction(crossParent, {
    transactionId: 'wrap-cross-parent', author: 'human', nodeIds: ['shape-first', 'text-third'],
    wrapperId: 'group-invalid', kind: 'group', name: 'Invalid'
  }), /sibling nodes/);

  const auto = freeScene();
  const autoRoot = indexSceneDocument(auto).get('frame-root').node;
  autoRoot.layout.mode = 'auto';
  autoRoot.layout.direction = 'vertical';
  autoRoot.layout.wrap = false;
  autoRoot.layout.alignItems = 'start';
  autoRoot.layout.justifyContent = 'start';
  assert.throws(() => createWrapSceneNodesTransaction(auto, {
    transactionId: 'wrap-auto', author: 'human', nodeIds: ['shape-first', 'shape-second'],
    wrapperId: 'group-auto', kind: 'group', name: 'Invalid auto'
  }), /free-layout container/);
});

test('eight-direction resize geometry keeps the opposite edge stable when minimum sizes clamp', () => {
  const frame = { x: 100, y: 80, width: 120, height: 60 };
  assert.deepEqual(resizeSceneRect(frame, 'east', 50, 0), { x: 100, y: 80, width: 170, height: 60 });
  assert.deepEqual(resizeSceneRect(frame, 'west', 200, 0, 40, 20), { x: 180, y: 80, width: 40, height: 60 });
  assert.deepEqual(resizeSceneRect(frame, 'north', 0, 100, 40, 20), { x: 100, y: 120, width: 120, height: 20 });
  assert.deepEqual(resizeSceneRect(frame, 'south-west', -30, 25), { x: 70, y: 80, width: 150, height: 85 });
});

test('Scene v2 resize transaction updates only affected axes and makes resized hug axes fixed', () => {
  const source = freeScene();
  const first = indexSceneDocument(source).get('shape-first').node;
  first.layout.sizingX = 'hug';
  const resized = applySceneTransaction(source, createResizeSceneNodeTransaction(source, {
    transactionId: 'resize-first-east', author: 'human', nodeId: 'shape-first',
    handle: 'east', deltaX: 45, deltaY: 999
  })).document;
  const node = indexSceneDocument(resized).get('shape-first').node;
  assert.deepEqual(node.frame, { x: 100, y: 80, width: 165, height: 60 });
  assert.equal(node.layout.sizingX, 'fixed');
  assert.equal(node.layout.sizingY, 'fixed');
});

test('Scene v2 multi-move transforms only selection roots and preserves nested geometry', () => {
  const source = freeScene();
  const wrapped = applySceneTransaction(source, createWrapSceneNodesTransaction(source, {
    transactionId: 'wrap-before-move', author: 'human', nodeIds: ['shape-first', 'shape-second'],
    wrapperId: 'group-movable', kind: 'group', name: 'Movable'
  })).document;
  const before = positions(wrapped, ['shape-first', 'shape-second']);
  const transaction = createMoveSceneNodesTransaction(wrapped, {
    transactionId: 'move-root-selection', author: 'human', nodeIds: ['group-movable', 'shape-first'], deltaX: 25, deltaY: -10
  });
  assert.deepEqual(transaction.operations.map((operation) => operation.nodeId), ['group-movable']);
  const moved = applySceneTransaction(wrapped, transaction).document;
  const after = positions(moved, ['shape-first', 'shape-second']);
  assert.deepEqual(after['shape-first'], { ...before['shape-first'], x: before['shape-first'].x + 25, y: before['shape-first'].y - 10 });
  assert.deepEqual(after['shape-second'], { ...before['shape-second'], x: before['shape-second'].x + 25, y: before['shape-second'].y - 10 });
});

test('Scene v2 alignment and distribution use atomic geometry transactions', () => {
  const source = freeScene();
  const aligned = applySceneTransaction(source, createAlignSceneNodesTransaction(source, {
    transactionId: 'align-left', author: 'human', nodeIds: ['shape-first', 'shape-second'], alignment: 'left'
  })).document;
  assert.equal(indexSceneDocument(aligned).get('shape-first').node.frame.x, 100);
  assert.equal(indexSceneDocument(aligned).get('shape-second').node.frame.x, 100);

  const distributed = applySceneTransaction(source, createDistributeSceneNodesTransaction(source, {
    transactionId: 'distribute-horizontal', author: 'human',
    nodeIds: ['shape-first', 'shape-second', 'text-third'], axis: 'horizontal'
  })).document;
  const first = indexSceneDocument(distributed).get('shape-first').node.frame;
  const second = indexSceneDocument(distributed).get('shape-second').node.frame;
  const third = indexSceneDocument(distributed).get('text-third').node.frame;
  assert.equal(second.x - (first.x + first.width), third.x - (second.x + second.width));
});

test('Scene v2 layer reordering preserves selected order and moves a selection block one step', () => {
  const source = freeScene();
  const front = applySceneTransaction(source, createReorderSceneNodesTransaction(source, {
    transactionId: 'layers-front', author: 'human', nodeIds: ['shape-first', 'shape-second'], placement: 'front'
  })).document;
  assert.deepEqual(indexSceneDocument(front).get('frame-root').node.children.map((node) => node.id), [
    'text-third', 'shape-first', 'shape-second'
  ]);
  const backward = applySceneTransaction(front, createReorderSceneNodesTransaction(front, {
    transactionId: 'layers-backward', author: 'human', nodeIds: ['shape-first', 'shape-second'], placement: 'backward'
  })).document;
  assert.deepEqual(indexSceneDocument(backward).get('frame-root').node.children.map((node) => node.id), [
    'shape-first', 'shape-second', 'text-third'
  ]);
});

test('Scene v2 free transforms reject flow children controlled by Auto Layout', () => {
  const source = freeScene();
  const root = indexSceneDocument(source).get('frame-root').node;
  root.layout.mode = 'auto';
  root.layout.direction = 'vertical';
  root.layout.wrap = false;
  root.layout.alignItems = 'start';
  root.layout.justifyContent = 'start';
  assert.throws(() => createMoveSceneNodesTransaction(source, {
    transactionId: 'move-flow-child', author: 'human', nodeIds: ['shape-first'], deltaX: 20, deltaY: 0
  }), /flow-positioned/);
});

test('Scene v2 Auto Layout Frame uses spatial order and inferred gap without shifting an aligned row', () => {
  const source = freeScene();
  const second = indexSceneDocument(source).get('shape-second').node;
  second.frame.y = 80;
  second.frame.height = 60;
  const before = positions(source, ['shape-first', 'shape-second']);
  const transaction = createAutoLayoutSceneFrameTransaction(source, {
    transactionId: 'auto-layout-row', author: 'human', nodeIds: ['shape-second', 'shape-first'],
    wrapperId: 'frame-auto-row', name: 'Auto row', direction: 'horizontal', padding: 10
  });
  const result = applySceneTransaction(source, transaction).document;
  assert.deepEqual(positions(result, ['shape-first', 'shape-second']), before);
  const frame = indexSceneDocument(result).get('frame-auto-row').node;
  assert.equal(frame.layout.mode, 'auto');
  assert.equal(frame.layout.direction, 'horizontal');
  assert.deepEqual(frame.layout.gap, { row: 60, column: 60 });
  assert.deepEqual(frame.children.map((node) => node.id), ['shape-first', 'shape-second']);
  assert.ok(frame.children.every((node) => node.layout.position === 'flow'));
});
