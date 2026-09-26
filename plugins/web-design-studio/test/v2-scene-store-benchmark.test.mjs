import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createRequire, syncBuiltinESMExports } from 'node:module';
import { mkdtemp, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import test from 'node:test';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

const require = createRequire(import.meta.url);
const zlib = require('node:zlib');
const originalGzipSync = zlib.gzipSync;
const originalGunzipSync = zlib.gunzipSync;
const codecMetrics = { compressionCount: 0, decompressionCount: 0 };
zlib.gzipSync = (...args) => {
  codecMetrics.compressionCount += 1;
  return originalGzipSync(...args);
};
zlib.gunzipSync = (...args) => {
  codecMetrics.decompressionCount += 1;
  return originalGunzipSync(...args);
};
syncBuiltinESMExports();
const { SceneDocumentStore } = await import('../dist/v2-scene-store.test.mjs');

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

async function recordSnapshot(root) {
  const files = await readdir(root);
  const fileName = files.find((candidate) => candidate.endsWith('.json'));
  assert.ok(fileName, 'Scene benchmark record is missing.');
  const file = path.join(root, fileName);
  const source = await readFile(file);
  return { file, source, record: JSON.parse(source.toString('utf8')) };
}

function instrumentWrites(store) {
  const metrics = { writeCount: 0, writtenBytes: 0 };
  const originalWrite = store.files.write.bind(store.files);
  store.files.write = async (fileName, value) => {
    metrics.writeCount += 1;
    metrics.writtenBytes += Buffer.byteLength(`${JSON.stringify(value, null, 2)}\n`, 'utf8');
    return originalWrite(fileName, value);
  };
  return metrics;
}

function metricSnapshot(writeMetrics) {
  return {
    compressionCount: codecMetrics.compressionCount,
    decompressionCount: codecMetrics.decompressionCount,
    writeCount: writeMetrics.writeCount,
    writtenBytes: writeMetrics.writtenBytes
  };
}

function metricDelta(before, after) {
  return Object.fromEntries(Object.keys(before).map((key) => [key, after[key] - before[key]]));
}

function percentile(values, percentileValue) {
  const sorted = values.slice().sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * percentileValue) - 1)];
}

function summarizeSamples(samples) {
  const timings = samples.map((sample) => sample.elapsedMs);
  return {
    sampleCount: samples.length,
    p50Ms: percentile(timings, 0.5),
    p95Ms: percentile(timings, 0.95),
    compressionCount: samples.reduce((total, sample) => total + sample.compressionCount, 0),
    decompressionCount: samples.reduce((total, sample) => total + sample.decompressionCount, 0),
    writeCount: samples.reduce((total, sample) => total + sample.writeCount, 0),
    writtenBytes: samples.reduce((total, sample) => total + sample.writtenBytes, 0),
    peakHeapBytes: Math.max(...samples.map((sample) => sample.peakHeapBytes))
  };
}

async function measureStableOperation(store, snapshot, writeMetrics, operation, label, sampleCount = 7) {
  const samples = [];
  for (let sample = 0; sample < sampleCount; sample += 1) {
    await writeFile(snapshot.file, snapshot.source);
    const baseline = snapshot.record.document;
    if (operation === 'redo') await store.undo(baseline.documentId, baseline.revision);
    const beforeMetrics = metricSnapshot(writeMetrics);
    const beforeHeapBytes = process.memoryUsage().heapUsed;
    const started = performance.now();
    if (operation === 'read') await store.read(baseline.documentId);
    if (operation === 'apply') {
      await store.apply(baseline.documentId, headlineTransaction(
        baseline.revision,
        deterministicNoise(256, `${label}-${sample}`),
        `benchmark-${label}-${sample}`
      ));
    }
    if (operation === 'undo') await store.undo(baseline.documentId, baseline.revision);
    if (operation === 'redo') await store.redo(baseline.documentId, baseline.revision + 1);
    const elapsedMs = performance.now() - started;
    const afterHeapBytes = process.memoryUsage().heapUsed;
    const delta = metricDelta(beforeMetrics, metricSnapshot(writeMetrics));
    samples.push({
      elapsedMs,
      peakHeapBytes: Math.max(beforeHeapBytes, afterHeapBytes),
      ...delta
    });
  }
  return summarizeSamples(samples);
}

async function measureStableProfile(store, root, label) {
  const snapshot = await recordSnapshot(root);
  const writeMetrics = instrumentWrites(store);
  const profile = {};
  for (const operation of ['read', 'apply', 'undo', 'redo']) {
    profile[operation] = await measureStableOperation(store, snapshot, writeMetrics, operation, `${label}-${operation}`);
  }
  return profile;
}

function assertCompleteProfile(profile) {
  for (const operation of ['read', 'apply', 'undo', 'redo']) {
    const metrics = profile[operation];
    assert.equal(metrics.sampleCount, 7);
    assert.ok(Number.isFinite(metrics.p50Ms));
    assert.ok(Number.isFinite(metrics.p95Ms));
    assert.ok(metrics.p95Ms >= metrics.p50Ms);
    assert.ok(Number.isSafeInteger(metrics.compressionCount));
    assert.ok(Number.isSafeInteger(metrics.decompressionCount));
    assert.ok(Number.isSafeInteger(metrics.writtenBytes));
    assert.ok(Number.isSafeInteger(metrics.peakHeapBytes));
  }
  assert.equal(profile.read.writeCount, 0);
  assert.equal(profile.read.writtenBytes, 0);
  assert.ok(profile.apply.compressionCount > 0);
  assert.ok(profile.apply.writtenBytes > 0);
  assert.ok(profile.undo.decompressionCount > 0);
  assert.ok(profile.redo.decompressionCount > 0);
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

test('scene store baseline records distribution, codec, write, and memory metrics without timing thresholds', async (context) => {
  const profiles = [];
  for (const historyCount of [1, 10, 60]) {
    const root = await mkdtemp(path.join(os.tmpdir(), `scene-store-profile-${historyCount}-`));
    try {
      const store = new SceneDocumentStore(root, 60, 64 * 1024 * 1024);
      await seedHistory(store, historyCount);
      const profile = await measureStableProfile(store, root, `history-${historyCount}`);
      assertCompleteProfile(profile);
      profiles.push({ historyCount, profile });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  }

  const byteLimitRoot = await mkdtemp(path.join(os.tmpdir(), 'scene-store-profile-byte-limit-'));
  const historyByteLimit = 128 * 1024;
  try {
    const store = new SceneDocumentStore(byteLimitRoot, 60, historyByteLimit);
    let document = await seedHistory(store, 1, 4096);
    let record = await readRecord(byteLimitRoot);
    for (let index = 1; storedHistoryBytes(record) < historyByteLimit * 0.8 && index < 60; index += 1) {
      document = (await store.apply(document.documentId, headlineTransaction(
        document.revision,
        deterministicNoise(4096, `metric-byte-limit-${index}`),
        `benchmark-metric-byte-limit-${index}`
      ))).document;
      record = await readRecord(byteLimitRoot);
    }
    const storedBytes = storedHistoryBytes(record);
    assert.ok(storedBytes >= historyByteLimit * 0.8);
    assert.ok(storedBytes <= historyByteLimit);
    const profile = await measureStableProfile(store, byteLimitRoot, 'byte-limit');
    assertCompleteProfile(profile);
    profiles.push({ historyByteLimit, storedBytes, historyCount: record.past.length, profile });
  } finally {
    await rm(byteLimitRoot, { recursive: true, force: true });
  }
  context.diagnostic(`scene-store-metric-baseline ${JSON.stringify(profiles)}`);
});
