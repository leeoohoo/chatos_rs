import assert from 'node:assert/strict';
import { mkdtemp, readdir, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { SceneDocumentStore, SceneRevisionConflictError } from '../dist/v2-scene-store.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

function headlineTransaction(baseRevision, content, transactionId = `transaction-${baseRevision}`) {
  return {
    transactionId,
    baseRevision,
    author: 'human',
    operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['content'], value: content }] }]
  };
}

test('v2 scene store persists atomic transactions with monotonic revisions', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-scene-store-'));
  const store = new SceneDocumentStore(root);
  try {
    const created = await store.create(nestedWebsite());
    assert.equal(created.revision, 1);
    const applied = await store.apply(created.documentId, headlineTransaction(1, 'Persisted headline', 'transaction-persist'));
    assert.equal(applied.document.revision, 2);
    assert.equal(indexSceneDocument(await store.read(created.documentId)).get('text-hero-heading').node.content, 'Persisted headline');
    await assert.rejects(
      () => store.apply(created.documentId, headlineTransaction(1, 'Stale headline', 'transaction-stale')),
      (error) => error instanceof SceneRevisionConflictError && error.actualRevision === 2
    );

    const files = await readdir(root);
    assert.equal(files.filter((file) => file.endsWith('.json')).length, 1);
    assert.equal(files.some((file) => file.endsWith('.tmp')), false);
    const record = JSON.parse(await readFile(path.join(root, files.find((file) => file.endsWith('.json'))), 'utf8'));
    assert.equal(record.document.revision, 2);
    assert.equal(record.past.length, 1);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('undo and redo restore exact content while revisions keep increasing', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-scene-history-'));
  const store = new SceneDocumentStore(root);
  try {
    const created = await store.create(nestedWebsite());
    const original = indexSceneDocument(created).get('text-hero-heading').node.content;
    const changed = await store.apply(created.documentId, headlineTransaction(1, 'Changed once', 'transaction-change'));
    assert.deepEqual(await store.history(created.documentId), { undoCount: 1, redoCount: 0, nextUndoTransactionId: 'transaction-change', nextRedoTransactionId: undefined });

    const undone = await store.undo(created.documentId, changed.document.revision);
    assert.equal(undone.revision, 3);
    assert.equal(indexSceneDocument(undone).get('text-hero-heading').node.content, original);
    assert.equal((await store.history(created.documentId)).nextRedoTransactionId, 'transaction-change');

    const redone = await store.redo(created.documentId, undone.revision);
    assert.equal(redone.revision, 4);
    assert.equal(indexSceneDocument(redone).get('text-hero-heading').node.content, 'Changed once');
    assert.equal((await store.history(created.documentId)).redoCount, 0);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('a new transaction after undo clears the redo branch', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-scene-branch-'));
  const store = new SceneDocumentStore(root);
  try {
    const created = await store.create(nestedWebsite());
    const first = await store.apply(created.documentId, headlineTransaction(1, 'First branch', 'transaction-first'));
    const undone = await store.undo(created.documentId, first.document.revision);
    await store.apply(created.documentId, headlineTransaction(undone.revision, 'Second branch', 'transaction-second'));
    assert.equal((await store.history(created.documentId)).redoCount, 0);
    await assert.rejects(() => store.redo(created.documentId, 4), /nothing to redo/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('the directory lock allows only one concurrent writer at the same revision', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-scene-concurrency-'));
  const store = new SceneDocumentStore(root);
  try {
    const created = await store.create(nestedWebsite());
    const attempts = await Promise.allSettled([
      store.apply(created.documentId, headlineTransaction(1, 'Writer A', 'transaction-writer-a')),
      store.apply(created.documentId, headlineTransaction(1, 'Writer B', 'transaction-writer-b'))
    ]);
    assert.equal(attempts.filter((attempt) => attempt.status === 'fulfilled').length, 1);
    const rejection = attempts.find((attempt) => attempt.status === 'rejected');
    assert.ok(rejection.reason instanceof SceneRevisionConflictError);
    assert.equal((await store.read(created.documentId)).revision, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('history uses checksummed compressed snapshots and enforces count and byte budgets', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-scene-compressed-history-'));
  const store = new SceneDocumentStore(root, 3, 6 * 1024);
  try {
    let document = await store.create(nestedWebsite());
    for (let index = 0; index < 8; index += 1) {
      const content = Array.from({ length: 700 }, (_, character) => String.fromCharCode(33 + ((character * 31 + index * 17) % 90))).join('');
      document = (await store.apply(document.documentId, headlineTransaction(document.revision, content, `transaction-budget-${index}`))).document;
    }
    const files = await readdir(root);
    const file = path.join(root, files.find((candidate) => candidate.endsWith('.json')));
    const record = JSON.parse(await readFile(file, 'utf8'));
    assert.equal(record.formatVersion, 2);
    assert.ok(record.past.length >= 1 && record.past.length <= 3);
    assert.equal(record.past.at(-1).transaction.transactionId, 'transaction-budget-7');
    assert.equal(record.past[0].before.encoding, 'gzip-base64');
    assert.match(record.past[0].before.sha256, /^[a-f0-9]{64}$/);
    assert.equal(typeof record.past[0].before.data, 'string');
    assert.equal((await store.history(document.documentId)).undoCount, record.past.length);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('history count trimming keeps the newest exact undo chain', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-scene-history-limit-'));
  const store = new SceneDocumentStore(root, 2);
  try {
    let document = await store.create(nestedWebsite());
    for (let index = 0; index < 4; index += 1) {
      document = (await store.apply(document.documentId, headlineTransaction(document.revision, `Version ${index}`, `transaction-limit-${index}`))).document;
    }
    assert.deepEqual(await store.history(document.documentId), {
      undoCount: 2,
      redoCount: 0,
      nextUndoTransactionId: 'transaction-limit-3',
      nextRedoTransactionId: undefined
    });
    document = await store.undo(document.documentId, document.revision);
    assert.equal(indexSceneDocument(document).get('text-hero-heading').node.content, 'Version 2');
    document = await store.undo(document.documentId, document.revision);
    assert.equal(indexSceneDocument(document).get('text-hero-heading').node.content, 'Version 1');
    await assert.rejects(() => store.undo(document.documentId, document.revision), /nothing to undo/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
