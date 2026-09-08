import assert from 'node:assert/strict';
import test from 'node:test';
import { createSceneNodeBase, indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { diffSceneDocuments } from '../dist/v2-scene-diff.test.mjs';
import { applySceneTransaction } from '../dist/v2-scene-transaction.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

function textNode(id, content) {
  return { ...createSceneNodeBase('text', content, { x: 0, y: 0, width: 180, height: 40 }), id, content };
}

test('scene diff reports exact field paths instead of only changed node ids', () => {
  const before = nestedWebsite();
  const after = applySceneTransaction(before, {
    transactionId: 'transaction-diff-fields',
    baseRevision: 0,
    author: 'human',
    operations: [{
      op: 'update-node',
      nodeId: 'text-hero-heading',
      patches: [
        { path: ['content'], value: 'A precise new headline' },
        { path: ['frame', 'width'], value: 640 },
        { path: ['appearance', 'typography', 'fontSize'], value: 72 }
      ]
    }]
  }, '2026-09-07T11:00:00.000Z').document;
  const diff = diffSceneDocuments(before, after);
  const headingChanges = diff.changes.filter((change) => change.kind === 'field-changed' && change.entityId === 'text-hero-heading');
  assert.deepEqual(headingChanges.map((change) => change.path), [
    ['appearance', 'typography', 'fontSize'],
    ['content'],
    ['frame', 'width'],
    ['updatedAt'],
    ['updatedBy']
  ]);
  assert.deepEqual(headingChanges.find((change) => change.path.join('.') === 'frame.width'), {
    kind: 'field-changed',
    entity: 'node',
    entityId: 'text-hero-heading',
    path: ['frame', 'width'],
    before: 560,
    after: 640
  });
  assert.ok(diff.changes.some((change) => change.kind === 'field-changed' && change.entity === 'document' && change.path[0] === 'revision'));
});

test('scene diff emits one subtree addition or removal instead of duplicating every descendant', () => {
  const before = nestedWebsite();
  const insertedGroup = {
    ...createSceneNodeBase('group', 'Feature group', { x: 0, y: 200, width: 400, height: 100 }),
    id: 'group-feature',
    children: [textNode('text-feature', 'Fast by default')]
  };
  const after = structuredClone(before);
  indexSceneDocument(after).get('frame-desktop').node.children.push(insertedGroup);
  after.revision = 1;
  const added = diffSceneDocuments(before, after).changes.filter((change) => change.kind === 'entity-added' && change.entity === 'node');
  assert.deepEqual(added.map((change) => change.entityId), ['group-feature']);
  assert.equal(added[0].value.children[0].id, 'text-feature');

  const removed = diffSceneDocuments(after, before).changes.filter((change) => change.kind === 'entity-removed' && change.entity === 'node');
  assert.deepEqual(removed.map((change) => change.entityId), ['group-feature']);
});

test('scene diff uses a minimal reorder signal and does not mark shifted siblings as moved', () => {
  const before = nestedWebsite();
  const frame = indexSceneDocument(before).get('frame-desktop').node;
  frame.children.push(textNode('text-second', 'Second'), textNode('text-third', 'Third'));
  const after = applySceneTransaction(before, {
    transactionId: 'transaction-diff-move',
    baseRevision: 0,
    author: 'human',
    operations: [{ op: 'move-node', nodeId: 'group-hero-copy', parentId: 'frame-desktop', index: 2 }]
  }).document;
  const moves = diffSceneDocuments(before, after).changes.filter((change) => change.kind === 'entity-moved' && change.entity === 'node');
  assert.deepEqual(moves.map((change) => change.entityId), ['group-hero-copy']);
  assert.equal(moves[0].before.index, 0);
  assert.equal(moves[0].after.index, 2);
});

test('scene diff detects cross-parent and slot moves without reporting descendant path noise', () => {
  const before = nestedWebsite();
  const target = {
    ...createSceneNodeBase('library-instance', 'Card', { x: 0, y: 0, width: 300, height: 200 }),
    id: 'library-card',
    library: 'antd',
    component: 'Card',
    properties: {},
    slots: { body: [] }
  };
  indexSceneDocument(before).get('frame-desktop').node.children.push(target);
  const after = applySceneTransaction(before, {
    transactionId: 'transaction-diff-slot-move',
    baseRevision: 0,
    author: 'human',
    operations: [{ op: 'move-node', nodeId: 'group-hero-copy', parentId: 'library-card', slot: 'body', index: 0 }]
  }).document;
  const moves = diffSceneDocuments(before, after).changes.filter((change) => change.kind === 'entity-moved' && change.entity === 'node');
  assert.deepEqual(moves.map((change) => change.entityId), ['group-hero-copy']);
  assert.equal(moves[0].after.parentId, 'library-card');
  assert.equal(moves[0].after.slot, 'body');
});

test('scene diff covers page, variable collection, variable, and nested annotation changes', () => {
  const before = nestedWebsite();
  const after = structuredClone(before);
  after.pages[0].name = 'Homepage';
  after.variableCollections[0].modes[0].name = 'Day';
  after.variableCollections[0].variables[0].valuesByMode['mode-light'] = '#FAFAFA';
  indexSceneDocument(after).get('text-hero-heading').node.annotations.push({
    id: 'annotation-one', author: 'human', body: 'Review this', status: 'open', createdAt: '2026-09-07T10:00:00.000Z'
  });
  const diff = diffSceneDocuments(before, after);
  assert.ok(diff.changes.some((change) => change.kind === 'field-changed' && change.entity === 'page' && change.path.join('.') === 'name'));
  assert.ok(diff.changes.some((change) => change.kind === 'field-changed' && change.entity === 'variable-collection' && change.path.join('.') === 'modes.0.name'));
  assert.ok(diff.changes.some((change) => change.kind === 'field-changed' && change.entity === 'variable' && change.path.join('.') === 'valuesByMode.mode-light'));
  assert.ok(diff.changes.some((change) => change.kind === 'field-changed' && change.entity === 'node' && change.path.join('.') === 'annotations.0'));
});

test('scene diff values are mutation-safe and identical documents produce no changes', () => {
  const before = nestedWebsite();
  assert.deepEqual(diffSceneDocuments(before, structuredClone(before)).changes, []);
  const after = structuredClone(before);
  indexSceneDocument(after).get('text-hero-heading').node.content = 'Changed';
  const diff = diffSceneDocuments(before, after);
  const change = diff.changes.find((candidate) => candidate.kind === 'field-changed' && candidate.path.join('.') === 'content');
  change.after = 'Mutated result';
  assert.equal(indexSceneDocument(after).get('text-hero-heading').node.content, 'Changed');
});
