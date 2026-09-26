import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, readdir, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import test from 'node:test';
import { SceneDocumentStore } from '../dist/v2-scene-store.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

function headlineTransaction(baseRevision, content, transactionId) {
  return {
    transactionId,
    baseRevision,
    author: 'human',
    operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['content'], value: content }] }]
  };
}

function deterministicNoise(length, salt) {
  let value = '';
  for (let index = 0; value.length < length; index += 1) {
    value += createHash('sha256').update(`${salt}:${index}`).digest('base64');
  }
  return value.slice(0, length);
}

function historyEntryBytes(entry) {
  return Buffer.byteLength(entry.before.data, 'utf8')
    + Buffer.byteLength(entry.after.data, 'utf8')
    + Buffer.byteLength(JSON.stringify(entry.transaction), 'utf8')
    + Buffer.byteLength(JSON.stringify(entry.summary), 'utf8');
}

function storedHistoryBytes(record) {
  return [...record.past, ...record.future]
    .reduce((total, entry) => total + historyEntryBytes(entry), 0);
}

async function readRecord(root) {
  const files = await readdir(root);
  const fileName = files.find((candidate) => candidate.endsWith('.json'));
  assert.ok(fileName, 'Scene benchmark record is missing.');
  return JSON.parse(await readFile(path.join(root, fileName), 'utf8'));
}

async function seedHistory(store, historyCount, contentBytes = 128) {
  let document = await store.create(nestedWebsite());
  for (let index = 0; index < historyCount; index += 1) {
    document = (await store.apply(document.documentId, headlineTransaction(
      document.revision,
      deterministicNoise(contentBytes, `seed-${historyCount}-${index}`),
      `benchmark-seed-${historyCount}-${index}`
    ))).document;
  }
  return document;
}

async function measureOperationCycle(store, document, label) {
  let started = performance.now();
  await store.read(document.documentId);
  const readMs = performance.now() - started;

  started = performance.now();
  const applied = await store.apply(document.documentId, headlineTransaction(
    document.revision,
    deterministicNoise(256, `${label}-apply`),
    `benchmark-${label}-apply`
  ));
  const applyMs = performance.now() - started;

  started = performance.now();
  const undone = await store.undo(document.documentId, applied.document.revision);
  const undoMs = performance.now() - started;

  started = performance.now();
  const redone = await store.redo(document.documentId, undone.revision);
  const redoMs = performance.now() - started;

  return { readMs, applyMs, undoMs, redoMs, revision: redone.revision };
}

test('scene store baseline covers 1, 10, and 60 history entries', async (context) => {
  const results = [];
  for (const historyCount of [1, 10, 60]) {
    const root = await mkdtemp(path.join(os.tmpdir(), `scene-store-benchmark-${historyCount}-`));
    try {
      const store = new SceneDocumentStore(root, 60, 64 * 1024 * 1024);
      const document = await seedHistory(store, historyCount);
      assert.equal((await store.history(document.documentId)).undoCount, historyCount);
      const timings = await measureOperationCycle(store, document, `history-${historyCount}`);
      assert.ok(Object.values(timings).every(Number.isFinite));
      results.push({ historyCount, ...timings });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  }
  context.diagnostic(`scene-store-history-baseline ${JSON.stringify(results)}`);
});

test('scene store baseline exercises read, apply, undo, and redo near its byte limit', async (context) => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'scene-store-byte-limit-benchmark-'));
  const historyByteLimit = 128 * 1024;
  try {
    const store = new SceneDocumentStore(root, 60, historyByteLimit);
    let document = await seedHistory(store, 1, 4096);
    let record = await readRecord(root);
    for (let index = 1; storedHistoryBytes(record) < historyByteLimit * 0.8 && index < 60; index += 1) {
      document = (await store.apply(document.documentId, headlineTransaction(
        document.revision,
        deterministicNoise(4096, `byte-limit-${index}`),
        `benchmark-byte-limit-${index}`
      ))).document;
      record = await readRecord(root);
    }

    const storedBytes = storedHistoryBytes(record);
    assert.ok(storedBytes >= historyByteLimit * 0.8, `History only reached ${storedBytes} of ${historyByteLimit} bytes.`);
    assert.ok(storedBytes <= historyByteLimit);
    const timings = await measureOperationCycle(store, document, 'byte-limit');
    assert.ok(Object.values(timings).every(Number.isFinite));
    context.diagnostic(`scene-store-byte-limit-baseline ${JSON.stringify({
      historyByteLimit,
      storedBytes,
      historyCount: record.past.length + record.future.length,
      ...timings
    })}`);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
