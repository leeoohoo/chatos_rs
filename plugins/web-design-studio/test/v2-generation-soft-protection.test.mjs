import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { GenerationSoftProtectionStore } from '../dist/v2-generation-soft-protection-store.test.mjs';
import { findGenerationSoftProtectionConflicts, softProtectionsFromHumanTransaction } from '../dist/v2-generation-soft-protection.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

const scope = { projectId: 'project-protection', documentId: 'scene-test' };

test('human updates, moves, and inserted nodes become field-level soft protections', () => {
  const transaction = {
    transactionId: 'human-composite-edit', baseRevision: 1, author: 'human',
    operations: [
      { op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['appearance', 'typography', 'fontSize'], value: 70 }] },
      { op: 'move-node', nodeId: 'group-hero-copy', parentId: 'frame-desktop', index: 0 },
      {
        op: 'insert-node', parentId: 'frame-desktop', index: 1,
        node: { ...nestedWebsite().pages[0].children[0].children[0].children[0], id: 'group-human-created', children: [] }
      }
    ]
  };
  const protections = softProtectionsFromHumanTransaction(transaction, 2);
  assert.ok(protections.some((item) => item.nodeId === 'text-hero-heading' && item.path.join('.') === 'appearance.typography.fontSize'));
  assert.ok(protections.some((item) => item.nodeId === 'group-hero-copy' && item.path.join('.') === 'parent'));
  assert.ok(protections.some((item) => item.nodeId === 'group-human-created' && item.path.join('.') === '$node'));
});

test('soft protection store is idempotent and filters entries against the current Scene', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-soft-protection-'));
  try {
    const store = new GenerationSoftProtectionStore(root);
    const transaction = {
      transactionId: 'human-font-edit', baseRevision: 1, author: 'human',
      operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['appearance', 'typography', 'fontSize'], value: 70 }] }]
    };
    await store.recordHumanTransaction(scope, transaction, 2);
    await store.recordHumanTransaction(scope, transaction, 2);
    const document = nestedWebsite();
    document.revision = 2;
    const protections = await new GenerationSoftProtectionStore(root).read(scope, document);
    assert.equal(protections.length, 1);
    assert.equal(protections[0].protectedAtRevision, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('AI changes touching a protected field or human-created node are reported for review', () => {
  const document = nestedWebsite();
  document.revision = 2;
  const protections = [
    { nodeId: 'text-hero-heading', path: ['appearance', 'typography', 'fontSize'], protectedAtRevision: 2, reason: 'Human typography edit' },
    { nodeId: 'group-hero-copy', path: ['$node'], protectedAtRevision: 2, reason: 'Human-created composition' }
  ];
  const transaction = {
    transactionId: 'ai-protected-edit', baseRevision: 2, author: 'ai',
    operations: [
      { op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['appearance', 'typography'], value: { fontSize: 80 } }] },
      { op: 'remove-node', nodeId: 'group-hero-copy' }
    ]
  };
  const conflicts = findGenerationSoftProtectionConflicts(document, transaction, protections);
  assert.equal(conflicts.length, 3);
  assert.deepEqual([...new Set(conflicts.map((item) => item.nodeId))].sort(), ['group-hero-copy', 'text-hero-heading']);
  assert.ok(conflicts.some((item) => item.nodeId === 'text-hero-heading' && item.requestedPath[0] === 'remove'));
});
