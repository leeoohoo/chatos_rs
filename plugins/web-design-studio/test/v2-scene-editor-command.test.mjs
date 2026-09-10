import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  executeSceneEditorCommand,
  parseSceneEditorCommand
} from '../dist/v2-scene-editor-command.test.mjs';
import { createBlankSceneDocument, createSceneNodeBase, indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { SceneDocumentStore } from '../dist/v2-scene-store.test.mjs';

function commandScene() {
  const document = createBlankSceneDocument('Editor commands');
  document.documentId = 'scene-editor-command';
  document.pages[0].id = 'page-home';
  document.pages[0].children = [{
    ...createSceneNodeBase('frame', 'Root', { x: 0, y: 0, width: 800, height: 600 }),
    id: 'frame-root',
    children: [
      { ...createSceneNodeBase('shape', 'First', { x: 40, y: 60, width: 120, height: 80 }), id: 'shape-first', shape: 'rectangle' },
      { ...createSceneNodeBase('shape', 'Second', { x: 220, y: 60, width: 120, height: 80 }), id: 'shape-second', shape: 'rectangle' }
    ]
  }];
  return document;
}

test('Scene editor command parser rejects unsupported fields and incomplete grouping', () => {
  assert.throws(() => parseSceneEditorCommand({
    transactionId: 'command:invalid-extra', expectedRevision: 1,
    command: { type: 'move', nodeIds: ['shape-first'], deltaX: 10, deltaY: 0, legacyX: 10 }
  }), /unsupported fields/);
  assert.throws(() => parseSceneEditorCommand({
    transactionId: 'command:invalid-group', expectedRevision: 1,
    command: { type: 'group', nodeIds: ['shape-first'], wrapperId: 'group-one', name: 'One' }
  }), /nodeIds is invalid/);
  assert.throws(() => parseSceneEditorCommand({
    transactionId: 'command:invalid-resize', expectedRevision: 1,
    command: { type: 'resize', nodeId: 'shape-first', handle: 'east', deltaX: 0, deltaY: 50 }
  }), /non-zero delta/);
  assert.throws(() => parseSceneEditorCommand({
    transactionId: 'command:invalid-property', expectedRevision: 1,
    command: { type: 'update-node', nodeId: 'shape-first', patches: [{ path: ['appearance', 'opacity'], value: 0.5 }] }
  }), /path is not editable/);
  assert.throws(() => parseSceneEditorCommand({
    transactionId: 'command:duplicate-property', expectedRevision: 1,
    command: { type: 'update-node', nodeId: 'shape-first', patches: [{ path: ['frame', 'x'], value: 10 }, { path: ['frame', 'x'], value: 20 }] }
  }), /duplicate path/);
});

test('Scene editor update-node command edits only whitelisted properties through history', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-update-'));
  try {
    const store = new SceneDocumentStore(root);
    const created = await store.create(commandScene());
    const updated = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:update-properties', expectedRevision: created.revision,
      command: {
        type: 'update-node', nodeId: 'shape-first', patches: [
          { path: ['name'], value: 'Primary shape' },
          { path: ['frame', 'width'], value: 180 },
          { path: ['layout', 'padding', 'left'], value: 12 },
          { path: ['locked'], value: true }
        ]
      }
    }, 'human');
    const node = indexSceneDocument(updated.document).get('shape-first').node;
    assert.equal(node.name, 'Primary shape');
    assert.equal(node.frame.width, 180);
    assert.equal(node.layout.padding.left, 12);
    assert.equal(node.locked, true);
    assert.equal(updated.summary.updatedNodeIds.includes('shape-first'), true);
    assert.equal((await store.history(created.documentId)).undoCount, 1);

    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:update-non-text-content', expectedRevision: updated.document.revision,
      command: { type: 'update-node', nodeId: 'shape-first', patches: [{ path: ['content'], value: 'Not valid here' }] }
    }, 'human'), /Only Scene text and library instance nodes/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Scene editor persists validated prototype links and clears them through the same command path', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-prototype-'));
  try {
    const source = commandScene();
    source.pages.push({ id: 'page-details', name: '详情页', children: [] });
    const store = new SceneDocumentStore(root);
    const created = await store.create(source);
    const linked = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:set-prototype-link', expectedRevision: created.revision,
      command: { type: 'update-node', nodeId: 'shape-first', patches: [{
        path: ['prototypeLink'], value: { trigger: 'click', action: 'navigate', targetPageId: 'page-details' }
      }] }
    }, 'human');
    assert.deepEqual(indexSceneDocument(linked.document).get('shape-first').node.prototypeLink, {
      trigger: 'click', action: 'navigate', targetPageId: 'page-details'
    });

    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:set-missing-prototype-link', expectedRevision: linked.document.revision,
      command: { type: 'update-node', nodeId: 'shape-first', patches: [{
        path: ['prototypeLink'], value: { trigger: 'click', action: 'overlay', targetPageId: 'page-missing' }
      }] }
    }, 'human'), /unknown page|not found/i);

    const cleared = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:clear-prototype-link', expectedRevision: linked.document.revision,
      command: { type: 'update-node', nodeId: 'shape-first', patches: [{ path: ['prototypeLink'], value: null }] }
    }, 'human');
    assert.equal(indexSceneDocument(cleared.document).get('shape-first').node.prototypeLink, undefined);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Scene editor creates, updates, and removes human responsive overrides without duplicating nodes', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-responsive-'));
  try {
    const store = new SceneDocumentStore(root);
    const created = await store.create(commandScene());
    const inserted = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:set-mobile-override', expectedRevision: created.revision,
      command: {
        type: 'set-responsive-override', ruleId: 'editor:mobile', ruleName: '人工手机布局', maxWidth: 768,
        nodeId: 'shape-first', override: { visible: false, layout: { sizingX: 'fill' } }
      }
    }, 'human');
    assert.equal(inserted.document.responsiveRules.length, 1);
    assert.deepEqual(inserted.document.responsiveRules[0].nodeOverrides, [{
      nodeId: 'shape-first', visible: false, layout: { sizingX: 'fill' }
    }]);
    assert.equal(indexSceneDocument(inserted.document).size, indexSceneDocument(created).size);

    const updated = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:update-mobile-override', expectedRevision: inserted.document.revision,
      command: {
        type: 'set-responsive-override', ruleId: 'editor:mobile', ruleName: '人工手机布局', maxWidth: 768,
        nodeId: 'shape-first', override: { visible: true, layout: { sizingX: 'hug' } }
      }
    }, 'human');
    assert.deepEqual(updated.document.responsiveRules[0].nodeOverrides[0], {
      nodeId: 'shape-first', visible: true, layout: { sizingX: 'hug' }
    });

    const cleared = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:clear-mobile-override', expectedRevision: updated.document.revision,
      command: { type: 'clear-responsive-override', ruleId: 'editor:mobile', nodeId: 'shape-first' }
    }, 'human');
    assert.equal(cleared.document.responsiveRules.length, 0);
    assert.deepEqual(cleared.summary.removedResponsiveRuleIds, ['editor:mobile']);
    const restored = await store.undo(created.documentId, cleared.document.revision);
    assert.equal(restored.responsiveRules[0].nodeOverrides[0].layout.sizingX, 'hug');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Scene editor annotation commands create server-authored timestamps and remain undoable', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-annotation-'));
  try {
    const store = new SceneDocumentStore(root);
    const created = await store.create(commandScene());
    const added = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:add-annotation', expectedRevision: created.revision,
      command: { type: 'add-annotation', nodeId: 'shape-first', annotationId: 'annotation:contrast', body: '  Increase the visual contrast.  ' }
    }, 'human');
    const annotation = indexSceneDocument(added.document).get('shape-first').node.annotations[0];
    assert.equal(annotation.id, 'annotation:contrast');
    assert.equal(annotation.author, 'human');
    assert.equal(annotation.body, 'Increase the visual contrast.');
    assert.equal(annotation.status, 'open');
    assert.equal(Number.isFinite(Date.parse(annotation.createdAt)), true);

    const resolved = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:resolve-annotation', expectedRevision: added.document.revision,
      command: { type: 'resolve-annotation', nodeId: 'shape-first', annotationId: annotation.id }
    }, 'human');
    assert.equal(indexSceneDocument(resolved.document).get('shape-first').node.annotations[0].status, 'resolved');

    const reopened = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:reopen-annotation', expectedRevision: resolved.document.revision,
      command: { type: 'reopen-annotation', nodeId: 'shape-first', annotationId: annotation.id }
    }, 'human');
    const reopenedAnnotation = indexSceneDocument(reopened.document).get('shape-first').node.annotations[0];
    assert.equal(reopenedAnnotation.status, 'open');
    assert.equal('resolvedAt' in reopenedAnnotation, false);

    const undone = await store.undo(created.documentId, reopened.document.revision);
    assert.equal(indexSceneDocument(undone).get('shape-first').node.annotations[0].status, 'resolved');
    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:ai-add-annotation', expectedRevision: undone.revision,
      command: { type: 'add-annotation', nodeId: 'shape-first', annotationId: 'annotation:ai', body: 'AI note' }
    }, 'ai'), /require|human annotation|human/i);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Scene editor creates and renames independent human artboards without legacy page data', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-page-'));
  try {
    const store = new SceneDocumentStore(root);
    const created = await store.create(commandScene());
    const inserted = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:create-modal-page', expectedRevision: created.revision,
      command: {
        type: 'create-page', pageId: 'page:confirm-modal', name: '确认弹窗',
        rootNodeId: 'root:confirm-modal', width: 720, height: 640
      }
    }, 'human');
    assert.equal(inserted.document.pages.length, 2);
    assert.deepEqual(inserted.summary.insertedPageIds, ['page:confirm-modal']);
    const page = inserted.document.pages[1];
    assert.equal(page.name, '确认弹窗');
    assert.equal(page.children[0].id, 'root:confirm-modal');
    assert.equal(page.children[0].role, 'page-root');
    assert.deepEqual(page.children[0].frame, { x: 0, y: 0, width: 720, height: 640 });
    assert.equal(page.children[0].layout.mode, 'auto');
    assert.equal(page.children[0].layout.sizingY, 'hug');
    assert.equal(page.children[0].layout.minHeight, 640);

    const renamed = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:rename-modal-page', expectedRevision: inserted.document.revision,
      command: { type: 'rename-page', pageId: page.id, name: '删除确认' }
    }, 'human');
    assert.equal(renamed.document.pages[1].name, '删除确认');
    assert.deepEqual(renamed.summary.renamedPageIds, [page.id]);
    const undone = await store.undo(created.documentId, renamed.document.revision);
    assert.equal(undone.pages[1].name, '确认弹窗');

    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:ai-create-page', expectedRevision: undone.revision,
      command: { type: 'create-page', pageId: 'page:ai', name: 'AI page', rootNodeId: 'root:ai', width: 800, height: 600 }
    }, 'ai'), /progressive page-start/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Scene editor duplicates and deletes complete artboards with responsive rules and history', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-page-copy-'));
  try {
    const store = new SceneDocumentStore(root);
    const source = commandScene();
    const first = indexSceneDocument(source).get('shape-first').node;
    first.annotations = [{
      id: 'annotation:source-only', author: 'human', body: 'Source note', status: 'open', createdAt: '2026-09-08T00:00:00.000Z'
    }];
    source.responsiveRules = [{
      id: 'responsive-mobile', name: 'Mobile', maxWidth: 480, variableModes: {},
      nodeOverrides: [{ nodeId: 'shape-first', visible: false }]
    }];
    first.prototypeLink = { trigger: 'click', action: 'navigate', targetPageId: 'page-home' };
    const created = await store.create(source);
    const duplicated = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:duplicate-page', expectedRevision: created.revision,
      command: { type: 'duplicate-page', pageId: 'page-home', newPageId: 'page-home-copy', name: '首页副本' }
    }, 'human');
    assert.deepEqual(duplicated.summary.insertedPageIds, ['page-home-copy']);
    assert.equal(duplicated.document.pages.length, 2);
    const copiedPage = duplicated.document.pages[1];
    const copiedIds = copiedPage.children.flatMap((node) => [node.id, ...node.children.map((child) => child.id)]);
    assert.equal(copiedIds.includes('frame-root'), false);
    const copiedOverride = duplicated.document.responsiveRules[0].nodeOverrides.find((override) => copiedIds.includes(override.nodeId));
    assert.ok(copiedOverride);
    assert.equal(indexSceneDocument(duplicated.document).get(copiedOverride.nodeId).node.annotations.length, 0);
    assert.equal(indexSceneDocument(duplicated.document).get(copiedOverride.nodeId).node.prototypeLink.targetPageId, 'page-home-copy');

    const deleted = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:delete-page', expectedRevision: duplicated.document.revision,
      command: { type: 'delete-page', pageId: 'page-home-copy' }
    }, 'human');
    assert.deepEqual(deleted.summary.removedPageIds, ['page-home-copy']);
    assert.equal(deleted.document.pages.length, 1);
    assert.equal(deleted.document.responsiveRules[0].nodeOverrides.length, 1);
    const restored = await store.undo(created.documentId, deleted.document.revision);
    assert.equal(restored.pages.length, 2);
    assert.equal(restored.responsiveRules[0].nodeOverrides.length, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('deleting a Scene page clears prototype links that target it and undo restores them', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-delete-prototype-'));
  try {
    const source = commandScene();
    source.pages.push({ id: 'page-modal', name: '确认弹窗', children: [] });
    indexSceneDocument(source).get('shape-first').node.prototypeLink = {
      trigger: 'click', action: 'overlay', targetPageId: 'page-modal'
    };
    const store = new SceneDocumentStore(root);
    const created = await store.create(source);
    const deleted = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:delete-prototype-target', expectedRevision: created.revision,
      command: { type: 'delete-page', pageId: 'page-modal' }
    }, 'human');
    assert.equal(indexSceneDocument(deleted.document).get('shape-first').node.prototypeLink, undefined);
    assert.deepEqual(deleted.summary.updatedNodeIds, ['shape-first']);
    const restored = await store.undo(created.documentId, deleted.document.revision);
    assert.equal(indexSceneDocument(restored).get('shape-first').node.prototypeLink.targetPageId, 'page-modal');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Scene editor inserts a native node for humans and preserves it through undo and redo', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-insert-'));
  try {
    const store = new SceneDocumentStore(root);
    const created = await store.create(commandScene());
    const node = {
      ...createSceneNodeBase('library-instance', 'Ant Design Button', { x: 90, y: 220, width: 180, height: 44 }, 'human'),
      id: 'library:button-primary',
      type: 'library-instance',
      library: 'antd',
      component: 'Button',
      variant: 'primary',
      content: '继续',
      properties: { componentSlug: 'button', type: 'primary' },
      slots: {}
    };
    const inserted = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:insert-library-node', expectedRevision: created.revision,
      command: { type: 'insert-node', parentId: 'frame-root', index: 2, node }
    }, 'human');
    assert.deepEqual(inserted.summary.insertedNodeIds, [node.id]);
    assert.equal(indexSceneDocument(inserted.document).get(node.id).node.component, 'Button');
    assert.equal(indexSceneDocument(inserted.document).get(node.id).node.createdBy, 'human');

    const undone = await store.undo(created.documentId, inserted.document.revision);
    assert.equal(indexSceneDocument(undone).has(node.id), false);
    const redone = await store.redo(created.documentId, undone.revision);
    assert.equal(indexSceneDocument(redone).get(node.id).node.content, '继续');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Scene editor creates a library content slot on first bounded insertion', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-library-slot-'));
  try {
    const store = new SceneDocumentStore(root);
    const source = commandScene();
    const card = {
      ...createSceneNodeBase('library-instance', 'Ant Design Card', { x: 80, y: 160, width: 420, height: 280 }),
      id: 'library:card', type: 'library-instance', library: 'antd', component: 'Card', variant: 'default',
      content: 'Card', properties: { componentSlug: 'card' }, slots: {}
    };
    indexSceneDocument(source).get('frame-root').node.children.push(card);
    const created = await store.create(source);
    const child = { ...createSceneNodeBase('text', 'Card title', { x: 24, y: 24, width: 240, height: 40 }), id: 'text:card-title', type: 'text', content: 'AI-first design' };
    const inserted = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:insert-library-slot-child', expectedRevision: created.revision,
      command: { type: 'insert-node', parentId: card.id, slot: 'content', index: 0, node: child }
    }, 'human');
    const changed = indexSceneDocument(inserted.document).get(card.id).node;
    assert.equal(changed.slots.content[0].id, child.id);
    assert.equal(indexSceneDocument(inserted.document).get(child.id).parentId, card.id);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Scene editor rejects AI direct insertion and invalid human insertion targets', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-insert-policy-'));
  try {
    const store = new SceneDocumentStore(root);
    const created = await store.create(commandScene());
    const node = { ...createSceneNodeBase('shape', 'New shape', { x: 0, y: 0, width: 80, height: 80 }), id: 'shape-new', shape: 'ellipse' };
    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:ai-direct-insert', expectedRevision: created.revision,
      command: { type: 'insert-node', parentId: 'frame-root', index: 0, node }
    }, 'ai'), /bounded progressive generation workflow/);
    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:missing-parent', expectedRevision: created.revision,
      command: { type: 'insert-node', parentId: 'frame-missing', index: 0, node }
    }, 'human'), /parent not found/);
    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:invalid-slot', expectedRevision: created.revision,
      command: { type: 'insert-node', parentId: 'frame-root', index: 0, slot: 'content', node }
    }, 'human'), /does not accept a slot/);
    const duplicate = { ...node, id: 'shape-first' };
    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:duplicate-insert', expectedRevision: created.revision,
      command: { type: 'insert-node', parentId: 'frame-root', index: 0, node: duplicate }
    }, 'human'), /Duplicate scene id/);
    assert.equal((await store.read(created.documentId)).revision, created.revision);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Scene editor can replace official library bindings only on library instances', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-library-binding-'));
  try {
    const store = new SceneDocumentStore(root);
    const source = commandScene();
    const libraryNode = {
      ...createSceneNodeBase('library-instance', 'Button', { x: 80, y: 200, width: 120, height: 40 }),
      id: 'library:button', type: 'library-instance', library: 'antd', component: 'Button', variant: 'default',
      content: 'Button', properties: { componentSlug: 'button' }, slots: {}
    };
    indexSceneDocument(source).get('frame-root').node.children.push(libraryNode);
    const created = await store.create(source);
    const updated = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:update-library-binding', expectedRevision: created.revision,
      command: {
        type: 'update-node', nodeId: libraryNode.id, patches: [
          { path: ['variant'], value: 'primary' },
          { path: ['content'], value: '开始设计' },
          { path: ['properties'], value: { componentSlug: 'button', type: 'primary' } }
        ]
      }
    }, 'human');
    const changed = indexSceneDocument(updated.document).get(libraryNode.id).node;
    assert.equal(changed.variant, 'primary');
    assert.equal(changed.content, '开始设计');
    assert.equal(changed.properties.type, 'primary');
    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:update-shape-library-binding', expectedRevision: updated.document.revision,
      command: { type: 'update-node', nodeId: 'shape-first', patches: [{ path: ['component'], value: 'Button' }] }
    }, 'human'), /Only Scene library instance nodes/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Scene editor delete-nodes command removes only selection roots and remains undoable', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-delete-'));
  try {
    const store = new SceneDocumentStore(root);
    const source = commandScene();
    const first = indexSceneDocument(source).get('shape-first').node;
    first.type = 'group';
    first.children = [{ ...createSceneNodeBase('shape', 'Nested', { x: 10, y: 10, width: 20, height: 20 }), id: 'shape-nested', shape: 'ellipse' }];
    delete first.shape;
    const created = await store.create(source);
    const deleted = await executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:delete-roots', expectedRevision: created.revision,
      command: { type: 'delete-nodes', nodeIds: ['shape-first', 'shape-nested'] }
    }, 'human');
    assert.equal(indexSceneDocument(deleted.document).has('shape-first'), false);
    assert.equal(indexSceneDocument(deleted.document).has('shape-nested'), false);
    assert.deepEqual(deleted.summary.removedNodeIds, ['shape-first', 'shape-nested']);
    const restored = await store.undo(created.documentId, deleted.document.revision);
    assert.equal(indexSceneDocument(restored).has('shape-first'), true);
    assert.equal(indexSceneDocument(restored).has('shape-nested'), true);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Scene editor command uses the store transaction path and recovers idempotent retries', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-'));
  try {
    const store = new SceneDocumentStore(root);
    const created = await store.create(commandScene());
    const request = {
      transactionId: 'command:group-selection', expectedRevision: created.revision,
      reason: 'Keep the paired visual elements together.',
      command: { type: 'group', nodeIds: ['shape-first', 'shape-second'], wrapperId: 'group-pair', name: 'Paired shapes' }
    };
    const applied = await executeSceneEditorCommand(store, created.documentId, request, 'human');
    assert.equal(applied.recovered, false);
    assert.equal(applied.document.revision, 2);
    assert.equal(applied.commandType, 'group');
    assert.deepEqual(indexSceneDocument(applied.document).get('group-pair').node.children.map((node) => node.id), ['shape-first', 'shape-second']);
    assert.deepEqual(applied.summary.insertedNodeIds, ['group-pair']);

    const retried = await executeSceneEditorCommand(store, created.documentId, request, 'human');
    assert.equal(retried.recovered, true);
    assert.equal(retried.document.revision, 2);
    assert.deepEqual(retried.summary, applied.summary);
    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      ...request,
      command: { type: 'frame', nodeIds: ['shape-first', 'shape-second'], wrapperId: 'frame-pair', name: 'Different request' }
    }, 'human'), /transaction id is already used/);
    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      ...request, expectedRevision: 2
    }, 'human'), /transaction id is already used/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('AI Scene editor commands preserve node protection policy', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-editor-command-protection-'));
  try {
    const store = new SceneDocumentStore(root);
    const source = commandScene();
    indexSceneDocument(source).get('shape-first').node.aiPolicy.editable = false;
    const created = await store.create(source);
    await assert.rejects(() => executeSceneEditorCommand(store, created.documentId, {
      transactionId: 'command:ai-locked-move', expectedRevision: created.revision,
      command: { type: 'move', nodeIds: ['shape-first'], deltaX: 10, deltaY: 0 }
    }, 'ai'), /AI cannot edit locked node/);
    assert.equal((await store.read(created.documentId)).revision, created.revision);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
