import assert from 'node:assert/strict';
import { performance } from 'node:perf_hooks';
import test from 'node:test';
import { assertSceneDocument, createBlankSceneDocument, createSceneNodeBase, indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { diffSceneDocuments } from '../dist/v2-scene-diff.test.mjs';
import { SceneQueryIndex } from '../dist/v2-scene-query.test.mjs';
import { applySceneTransaction } from '../dist/v2-scene-transaction.test.mjs';

function largeSceneDocument(nodeCount = 3000) {
  const document = createBlankSceneDocument('Large AI website');
  document.documentId = 'scene-large-performance';
  document.pages[0].id = 'page-large';
  const children = Array.from({ length: nodeCount }, (_, index) => ({
    ...createSceneNodeBase('text', `Copy ${index}`, { x: 0, y: index * 28, width: 480, height: 24 }, index % 3 === 0 ? 'ai' : 'human'),
    id: `text-large-${index}`,
    role: index % 25 === 0 ? 'call-to-action' : 'body-copy',
    content: `Website copy block ${index}`
  }));
  const frame = {
    ...createSceneNodeBase('frame', 'Long landing page', { x: 0, y: 0, width: 1440, height: nodeCount * 28 }),
    id: 'frame-large',
    children
  };
  document.pages[0].children = [frame];
  return document;
}

test('large scene validation, indexed query, transaction, and diff stay bounded', () => {
  const document = largeSceneDocument();
  const started = performance.now();
  assertSceneDocument(document);
  const validatedAt = performance.now();

  const queryIndex = new SceneQueryIndex(document);
  const callsToAction = queryIndex.query({ types: ['text'], roles: ['call-to-action'], createdBy: ['ai'] });
  const queriedAt = performance.now();
  assert.equal(callsToAction.length, 40);

  const updated = applySceneTransaction(document, {
    transactionId: 'transaction-large-update',
    baseRevision: 0,
    author: 'human',
    operations: [{ op: 'update-node', nodeId: 'text-large-2999', patches: [{ path: ['content'], value: 'Updated final block' }] }]
  }).document;
  const transactedAt = performance.now();
  assert.equal(indexSceneDocument(updated).get('text-large-2999').node.content, 'Updated final block');

  const reordered = structuredClone(updated);
  const frame = indexSceneDocument(reordered).get('frame-large').node;
  frame.children.push(frame.children.shift());
  const diff = diffSceneDocuments(updated, reordered);
  const diffedAt = performance.now();
  assert.deepEqual(diff.changes.filter((change) => change.kind === 'entity-moved').map((change) => change.entityId), ['text-large-0']);

  const timings = {
    validation: validatedAt - started,
    indexedQuery: queriedAt - validatedAt,
    transaction: transactedAt - queriedAt,
    diff: diffedAt - transactedAt,
    total: diffedAt - started
  };
  assert.ok(timings.validation < 2500, `Large scene validation took ${timings.validation.toFixed(1)}ms`);
  assert.ok(timings.indexedQuery < 2500, `Large scene query took ${timings.indexedQuery.toFixed(1)}ms`);
  assert.ok(timings.transaction < 4000, `Large scene transaction took ${timings.transaction.toFixed(1)}ms`);
  assert.ok(timings.diff < 4000, `Large scene diff took ${timings.diff.toFixed(1)}ms`);
  assert.ok(timings.total < 9000, `Large scene workflow took ${timings.total.toFixed(1)}ms`);
});
