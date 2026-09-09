import assert from 'node:assert/strict';
import test from 'node:test';
import { indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { applySceneTransaction } from '../dist/v2-scene-transaction.test.mjs';
import {
  createAnnotationAiTask,
  createAnnotationResolutionTransaction,
  validateAnnotationAiTransaction
} from '../dist/v2-annotation-ai-protocol.test.mjs';
import {
  createVisualRepairRequest,
  validateVisualRepairTransaction
} from '../dist/v2-visual-quality.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

function annotatedDocument() {
  const document = nestedWebsite();
  indexSceneDocument(document).get('group-hero-copy').node.annotations.push({
    id: 'annotation-spacing',
    author: 'human',
    body: 'Reduce the width of this hero group without changing the approved headline copy.',
    status: 'open',
    createdAt: '2026-09-08T05:00:00.000Z'
  });
  return document;
}

function taskFor(document) {
  return createAnnotationAiTask(document, {
    scope: { projectId: 'project-annotation', documentId: document.documentId },
    targetNodeId: 'group-hero-copy',
    annotationId: 'annotation-spacing',
    dependencyNodeIds: ['frame-desktop'],
    requiredViewportWidths: [390, 1440]
  });
}

test('annotation tasks preserve scope and expose only the target subtree plus explicit dependencies', () => {
  const document = annotatedDocument();
  const task = taskFor(document);
  assert.deepEqual(task.scope, { projectId: 'project-annotation', documentId: 'scene-test' });
  assert.equal(task.pageId, 'page-home');
  assert.deepEqual(task.targetSubtreeNodeIds, ['group-hero-copy', 'text-hero-heading']);
  assert.deepEqual(task.dependencyNodeIds, ['frame-desktop']);
  assert.deepEqual(task.allowedExistingNodeIds, ['group-hero-copy', 'text-hero-heading', 'frame-desktop']);
  assert.deepEqual(task.lockedFieldsByNodeId, { 'text-hero-heading': ['content'] });
  assert.equal(task.instruction, 'Reduce the width of this hero group without changing the approved headline copy.');
});

test('the shared scope validator allows targeted edits but blocks unrelated nodes, page changes, and annotation self-resolution', () => {
  const document = annotatedDocument();
  const task = taskFor(document);
  const valid = {
    transactionId: 'annotation-valid', baseRevision: 0, author: 'ai',
    operations: [
      { op: 'update-node', nodeId: 'group-hero-copy', patches: [{ path: ['frame', 'width'], value: 520 }] },
      { op: 'update-node', nodeId: 'frame-desktop', patches: [{ path: ['layout', 'gap', 'row'], value: 24 }] }
    ]
  };
  const scopeValidation = validateAnnotationAiTransaction(task, document, valid);
  assert.deepEqual(scopeValidation.affectedNodeIds, ['group-hero-copy', 'frame-desktop']);

  assert.throws(() => validateAnnotationAiTransaction(task, document, {
    transactionId: 'annotation-outside', baseRevision: 0, author: 'ai',
    operations: [{ op: 'update-node', nodeId: 'section-responsive', patches: [{ path: ['frame', 'width'], value: 900 }] }]
  }), /outside the allowed node scope/);
  assert.throws(() => validateAnnotationAiTransaction(task, document, {
    transactionId: 'annotation-page', baseRevision: 0, author: 'ai',
    operations: [{ op: 'rename-page', pageId: 'page-home', name: 'Changed' }]
  }), /cannot perform rename-page/);
  assert.throws(() => validateAnnotationAiTransaction(task, document, {
    transactionId: 'annotation-resolve-itself', baseRevision: 0, author: 'ai',
    operations: [{ op: 'update-node', nodeId: 'group-hero-copy', patches: [{ path: ['annotations'], value: [] }] }]
  }), /cannot patch annotations/);
  assert.throws(() => validateAnnotationAiTransaction(task, document, {
    transactionId: 'annotation-remove-root', baseRevision: 0, author: 'ai',
    operations: [{ op: 'remove-node', nodeId: 'group-hero-copy' }]
  }), /cannot remove target root/);
});

test('field-level human locks remain enforced after an annotation transaction passes scope validation', () => {
  const document = annotatedDocument();
  const task = taskFor(document);
  const transaction = {
    transactionId: 'annotation-locked-copy', baseRevision: 0, author: 'ai',
    operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['content'], value: 'AI overwrite' }] }]
  };
  validateAnnotationAiTransaction(task, document, transaction);
  assert.throws(() => applySceneTransaction(document, transaction), /human-locked field content/);
});

test('an annotation resolves only after an in-scope change passes layout, snapshot, and browser calibration', () => {
  const document = annotatedDocument();
  const task = taskFor(document);
  const transaction = {
    transactionId: 'annotation-complete', baseRevision: 0, author: 'ai',
    operations: [{ op: 'update-node', nodeId: 'group-hero-copy', patches: [{ path: ['frame', 'width'], value: 520 }] }]
  };
  const scopeValidation = validateAnnotationAiTransaction(task, document, transaction);
  const applied = applySceneTransaction(document, transaction, '2026-09-08T05:01:00.000Z');
  const validation = {
    taskId: task.taskId,
    transactionId: transaction.transactionId,
    scope: task.scope,
    revision: applied.document.revision,
    passed: true,
    remainingIssueIds: [],
    viewports: [
      { viewportWidth: 390, layoutPassed: true, snapshotId: 'snapshot-mobile', calibrationPassed: true },
      { viewportWidth: 1440, layoutPassed: true, snapshotId: 'snapshot-desktop', calibrationPassed: true }
    ]
  };
  assert.throws(() => createAnnotationResolutionTransaction(task, applied.document, scopeValidation, applied.summary, {
    ...validation,
    viewports: validation.viewports.slice(0, 1)
  }), /every required viewport/);
  const resolution = createAnnotationResolutionTransaction(task, applied.document, scopeValidation, applied.summary, validation, '2026-09-08T05:02:00.000Z');
  assert.equal(resolution.author, 'system');
  const resolved = applySceneTransaction(applied.document, resolution, '2026-09-08T05:02:00.000Z').document;
  assert.deepEqual(indexSceneDocument(resolved).get('group-hero-copy').node.annotations[0], {
    id: 'annotation-spacing', author: 'human',
    body: 'Reduce the width of this hero group without changing the approved headline copy.',
    status: 'resolved', createdAt: '2026-09-08T05:00:00.000Z', resolvedAt: '2026-09-08T05:02:00.000Z'
  });
});

test('visual repairs reuse the same scope validator and may edit descendants but not sibling or document state', () => {
  const document = nestedWebsite();
  const report = {
    reportId: 'quality:scene-test:0',
    scope: { projectId: 'project-repair', documentId: 'scene-test' },
    revision: 0, score: 90, passed: false, evaluatedViewportWidths: [390, 1440],
    issues: [{
      issueId: 'layout:group-hero-copy', criterion: 'layout', severity: 'error',
      message: 'Hero group needs repair.', nodeIds: ['group-hero-copy'], autoRepairable: true,
      suggestedAction: 'Repair the group.'
    }]
  };
  const request = createVisualRepairRequest(report, document);
  const valid = {
    transactionId: 'repair-descendant', baseRevision: 0, author: 'ai',
    operations: [{ op: 'update-node', nodeId: 'text-hero-heading', patches: [{ path: ['frame', 'height'], value: 140 }] }]
  };
  assert.deepEqual(validateVisualRepairTransaction(request, report, document, valid).affectedNodeIds, ['text-hero-heading']);
  assert.throws(() => validateVisualRepairTransaction(request, report, document, {
    transactionId: 'repair-outside', baseRevision: 0, author: 'ai',
    operations: [{ op: 'update-node', nodeId: 'frame-desktop', patches: [{ path: ['frame', 'height'], value: 800 }] }]
  }), /outside the allowed node scope/);
  assert.throws(() => validateVisualRepairTransaction(request, report, document, {
    transactionId: 'repair-variable', baseRevision: 0, author: 'ai',
    operations: [{ op: 'insert-variable-collection', index: 1, collection: document.variableCollections[0] }]
  }), /cannot perform insert-variable-collection/);
});
