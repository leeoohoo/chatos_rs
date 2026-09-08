import assert from 'node:assert/strict';
import test from 'node:test';
import { createSceneNodeBase, indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { applySceneTransaction } from '../dist/v2-scene-transaction.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

function textNode(id, content) {
  return { ...createSceneNodeBase('text', content, { x: 0, y: 0, width: 180, height: 40 }), id, content };
}

test('scene transactions apply multiple operations atomically without mutating the source', () => {
  const source = nestedWebsite();
  const before = JSON.stringify(source);
  const result = applySceneTransaction(source, {
    transactionId: 'transaction-human-1',
    baseRevision: 0,
    author: 'human',
    reason: 'Refine hero content',
    operations: [
      { op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['content'], value: 'A human-edited headline' }, { path: ['frame', 'width'], value: 620 }] },
      { op: 'insert-node', parentId: 'frame-desktop', index: 1, node: textNode('text-hero-supporting', 'Supporting copy') },
      { op: 'rename-page', pageId: 'page-home', name: 'Homepage' }
    ]
  }, '2026-09-07T10:00:00.000Z');

  assert.equal(JSON.stringify(source), before);
  assert.equal(result.document.revision, 1);
  assert.equal(result.document.pages[0].name, 'Homepage');
  assert.equal(indexSceneDocument(result.document).get('text-hero-heading').node.content, 'A human-edited headline');
  assert.equal(indexSceneDocument(result.document).get('text-hero-heading').node.frame.width, 620);
  assert.equal(indexSceneDocument(result.document).get('text-hero-supporting').node.createdBy, 'human');
  assert.deepEqual(result.summary.insertedNodeIds, ['text-hero-supporting']);
  assert.deepEqual(result.summary.updatedNodeIds, ['text-hero-heading']);
});

test('AI cannot overwrite human-locked fields or protection policy', () => {
  const source = nestedWebsite();
  assert.throws(() => applySceneTransaction(source, {
    transactionId: 'transaction-ai-content',
    baseRevision: 0,
    author: 'ai',
    operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['content'], value: 'Overwritten' }] }]
  }), /human-locked field content/);

  assert.throws(() => applySceneTransaction(source, {
    transactionId: 'transaction-ai-policy',
    baseRevision: 0,
    author: 'ai',
    operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['aiPolicy', 'lockedFields'], value: [] }] }]
  }), /cannot change protection policy/);

  const allowed = applySceneTransaction(source, {
    transactionId: 'transaction-ai-frame',
    baseRevision: 0,
    author: 'ai',
    operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['frame', 'width'], value: 600 }] }]
  });
  assert.equal(indexSceneDocument(allowed.document).get('text-hero-heading').node.frame.width, 600);
});

test('failed scene transactions leave every earlier operation unapplied', () => {
  const source = nestedWebsite();
  const before = JSON.stringify(source);
  assert.throws(() => applySceneTransaction(source, {
    transactionId: 'transaction-invalid-batch',
    baseRevision: 0,
    author: 'human',
    operations: [
      { op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['frame', 'x'], value: 250 }] },
      { op: 'insert-node', parentId: 'frame-desktop', index: 1, node: textNode('text-hero-heading', 'Duplicate id') }
    ]
  }), /Duplicate scene id/);
  assert.equal(JSON.stringify(source), before);
});

test('move operations use the final destination index and reject descendant cycles', () => {
  const source = nestedWebsite();
  const frame = indexSceneDocument(source).get('frame-desktop').node;
  frame.children.push(textNode('text-second', 'Second'), textNode('text-third', 'Third'));
  const moved = applySceneTransaction(source, {
    transactionId: 'transaction-reorder',
    baseRevision: 0,
    author: 'human',
    operations: [{ op: 'move-node', nodeId: 'group-hero-copy', parentId: 'frame-desktop', index: 2 }]
  });
  assert.deepEqual(indexSceneDocument(moved.document).get('frame-desktop').node.children.map((node) => node.id), ['text-second', 'text-third', 'group-hero-copy']);

  assert.throws(() => applySceneTransaction(source, {
    transactionId: 'transaction-cycle',
    baseRevision: 0,
    author: 'human',
    operations: [{ op: 'move-node', nodeId: 'frame-desktop', parentId: 'group-hero-copy', index: 0 }]
  }), /cannot move into its own subtree/);
});

test('AI cannot remove or move a subtree containing protected human work', () => {
  const source = nestedWebsite();
  assert.throws(() => applySceneTransaction(source, {
    transactionId: 'transaction-ai-remove',
    baseRevision: 0,
    author: 'ai',
    operations: [{ op: 'remove-node', nodeId: 'group-hero-copy' }]
  }), /cannot remove protected subtree/);
  const heading = indexSceneDocument(source).get('text-hero-heading').node;
  heading.locked = true;
  assert.throws(() => applySceneTransaction(source, {
    transactionId: 'transaction-ai-move',
    baseRevision: 0,
    author: 'ai',
    operations: [{ op: 'move-node', nodeId: 'group-hero-copy', parentId: 'frame-desktop', index: 0 }]
  }), /cannot move locked node/);
});

test('scene transactions reject stale revisions and unsafe patch paths', () => {
  const source = nestedWebsite();
  assert.throws(() => applySceneTransaction(source, {
    transactionId: 'transaction-stale',
    baseRevision: 4,
    author: 'human',
    operations: [{ op: 'rename-page', pageId: 'page-home', name: 'Stale' }]
  }), /Current revision is 0/);
  assert.throws(() => applySceneTransaction(source, {
    transactionId: 'transaction-prototype',
    baseRevision: 0,
    author: 'human',
    operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['__proto__', 'polluted'], value: true }] }]
  }), /patch path is invalid/);
});

test('design-system variable collections are inserted atomically with the scene revision', () => {
  const source = nestedWebsite();
  const result = applySceneTransaction(source, {
    transactionId: 'transaction-insert-design-system',
    baseRevision: source.revision,
    author: 'ai',
    operations: [{
      op: 'insert-variable-collection',
      index: 0,
      collection: {
        id: 'variables-accent',
        name: 'Accent',
        modes: [{ id: 'mode-light', name: 'Light' }],
        variables: [{ id: 'variable-brand-primary', name: 'Primary', type: 'color', valuesByMode: { 'mode-light': '#3157ff' } }]
      }
    }]
  }, '2026-09-08T01:00:00.000Z');
  assert.equal(result.document.revision, source.revision + 1);
  assert.deepEqual(result.summary.insertedVariableCollectionIds, ['variables-accent']);
  assert.equal(result.document.variableCollections[0].variables[0].valuesByMode['mode-light'], '#3157ff');
  assert.equal(source.variableCollections.length, 1);
  assert.throws(() => applySceneTransaction(source, {
    transactionId: 'transaction-invalid-design-system',
    baseRevision: source.revision,
    author: 'ai',
    operations: [{
      op: 'insert-variable-collection',
      index: 0,
      collection: { id: 'variables-brand', name: 'Brand', modes: [], variables: [] }
    }]
  }), /modes are invalid/);
});

test('responsive rules are inserted atomically after their target nodes exist', () => {
  const source = nestedWebsite();
  const rule = {
    id: 'responsive-mobile', name: 'Mobile stack', maxWidth: 700, variableModes: {},
    nodeOverrides: [{ nodeId: 'frame-desktop', layout: { padding: { top: 24, right: 20, bottom: 24, left: 20 } } }]
  };
  const result = applySceneTransaction(source, {
    transactionId: 'insert-responsive-rule', baseRevision: 0, author: 'ai',
    operations: [{ op: 'insert-responsive-rule', index: 0, rule }]
  });
  assert.deepEqual(result.document.responsiveRules, [rule]);
  assert.deepEqual(result.summary.insertedResponsiveRuleIds, ['responsive-mobile']);
  assert.throws(() => applySceneTransaction(source, {
    transactionId: 'insert-invalid-responsive-rule', baseRevision: 0, author: 'ai',
    operations: [{ op: 'insert-responsive-rule', index: 0, rule: { ...rule, id: 'responsive-missing', nodeOverrides: [{ nodeId: 'missing-node', visible: false }] } }]
  }), /unknown node missing-node/);
});
