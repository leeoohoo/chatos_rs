import { indexSceneDocument, type SceneDocument } from './scene-schema.js';
import type { SceneTransaction, SceneTransactionSummary } from './scene-transaction.js';
import type { DesignScope } from './design-protocol.js';
import {
  collectSceneSubtreeNodeIds,
  validateScopedAiTransaction,
  type ScopedAiTransactionValidation
} from './scoped-ai-transaction.js';

export interface AnnotationAiTask {
  schemaVersion: 1;
  taskId: string;
  scope: DesignScope;
  annotationId: string;
  targetNodeId: string;
  instruction: string;
  baseRevision: number;
  requiredViewportWidths: number[];
  targetSubtreeNodeIds: string[];
  dependencyNodeIds: string[];
  allowedExistingNodeIds: string[];
  fullyProtectedNodeIds: string[];
  lockedFieldsByNodeId: Record<string, string[]>;
}

export interface AnnotationAiTaskInput {
  scope: DesignScope;
  targetNodeId: string;
  annotationId: string;
  dependencyNodeIds?: string[];
  requiredViewportWidths: number[];
}

export interface AnnotationAiValidationArtifact {
  taskId: string;
  transactionId: string;
  scope: DesignScope;
  revision: number;
  passed: boolean;
  remainingIssueIds: string[];
  viewports: Array<{
    viewportWidth: number;
    layoutPassed: boolean;
    snapshotId: string;
    calibrationPassed: boolean;
  }>;
}

function assertIdentifier(value: string, label: string): void {
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/.test(value)) throw new Error(`${label} is invalid.`);
}

function assertScope(scope: DesignScope, document: SceneDocument): void {
  if (!scope || typeof scope !== 'object') throw new Error('Annotation AI scope is invalid.');
  assertIdentifier(scope.projectId, 'scope.projectId');
  assertIdentifier(scope.documentId, 'scope.documentId');
  if (scope.documentId !== document.documentId) throw new Error('Annotation AI task documentId does not match the Scene document.');
}

function assertViewportWidths(widths: number[]): void {
  if (!Array.isArray(widths) || widths.length === 0 || new Set(widths).size !== widths.length
    || widths.some((width) => !Number.isSafeInteger(width) || width < 240 || width > 10000)) {
    throw new Error('Annotation AI task needs unique valid viewport widths.');
  }
}

export function createAnnotationAiTask(document: SceneDocument, input: AnnotationAiTaskInput): AnnotationAiTask {
  assertScope(input.scope, document);
  assertViewportWidths(input.requiredViewportWidths);
  const index = indexSceneDocument(document);
  const target = index.get(input.targetNodeId)?.node;
  if (!target) throw new Error(`Annotation target node not found: ${input.targetNodeId}`);
  const annotation = target.annotations.find((candidate) => candidate.id === input.annotationId);
  if (!annotation) throw new Error(`Open annotation not found on target node: ${input.annotationId}`);
  if (annotation.status !== 'open') throw new Error(`Annotation ${input.annotationId} is already resolved.`);
  if (annotation.author !== 'human') throw new Error('Only a human annotation can create a human-review AI task.');
  const targetSubtreeNodeIds = collectSceneSubtreeNodeIds(document, [target.id]);
  const dependencyNodeIds = input.dependencyNodeIds ?? [];
  if (new Set(dependencyNodeIds).size !== dependencyNodeIds.length) throw new Error('Annotation dependency node IDs must be unique.');
  for (const dependencyNodeId of dependencyNodeIds) {
    if (!index.has(dependencyNodeId)) throw new Error(`Annotation dependency node not found: ${dependencyNodeId}`);
    if (targetSubtreeNodeIds.includes(dependencyNodeId)) throw new Error(`Annotation dependency ${dependencyNodeId} is already inside the target subtree.`);
  }
  const allowedExistingNodeIds = [...targetSubtreeNodeIds, ...dependencyNodeIds];
  const fullyProtectedNodeIds: string[] = [];
  const lockedFieldsByNodeId: Record<string, string[]> = {};
  for (const nodeId of allowedExistingNodeIds) {
    const node = index.get(nodeId)!.node;
    if (node.locked || !node.aiPolicy.editable) fullyProtectedNodeIds.push(nodeId);
    if (node.aiPolicy.lockedFields.length > 0) lockedFieldsByNodeId[nodeId] = [...node.aiPolicy.lockedFields];
  }
  return {
    schemaVersion: 1,
    taskId: `annotation-task:${document.documentId}:${annotation.id}:${document.revision}`,
    scope: structuredClone(input.scope),
    annotationId: annotation.id,
    targetNodeId: target.id,
    instruction: annotation.body,
    baseRevision: document.revision,
    requiredViewportWidths: [...input.requiredViewportWidths],
    targetSubtreeNodeIds,
    dependencyNodeIds: [...dependencyNodeIds],
    allowedExistingNodeIds,
    fullyProtectedNodeIds,
    lockedFieldsByNodeId
  };
}

export function validateAnnotationAiTransaction(
  task: AnnotationAiTask,
  document: SceneDocument,
  transaction: SceneTransaction
): ScopedAiTransactionValidation {
  assertScope(task.scope, document);
  if (task.baseRevision !== document.revision) throw new Error('Annotation AI task is stale.');
  const target = indexSceneDocument(document).get(task.targetNodeId)?.node;
  const annotation = target?.annotations.find((candidate) => candidate.id === task.annotationId);
  if (!annotation || annotation.status !== 'open') throw new Error('Annotation AI task no longer has an open target annotation.');
  return validateScopedAiTransaction(document, transaction, {
    baseRevision: task.baseRevision,
    allowedExistingNodeIds: task.allowedExistingNodeIds,
    targetRootNodeIds: [task.targetNodeId],
    dependencyNodeIds: task.dependencyNodeIds,
    preserveTargetRoots: true,
    forbiddenPatchRoots: ['annotations', 'aiPolicy', 'locked']
  });
}

function assertValidationArtifact(task: AnnotationAiTask, document: SceneDocument, validation: AnnotationAiValidationArtifact): void {
  if (validation.taskId !== task.taskId || validation.scope.projectId !== task.scope.projectId || validation.scope.documentId !== task.scope.documentId) {
    throw new Error('Annotation validation artifact belongs to another task or scope.');
  }
  if (validation.revision !== document.revision) throw new Error('Annotation validation artifact is stale.');
  if (!validation.passed || validation.remainingIssueIds.length > 0) throw new Error('Annotation cannot resolve before visual validation passes.');
  const widths = validation.viewports.map((viewport) => viewport.viewportWidth);
  if (widths.length !== task.requiredViewportWidths.length || new Set(widths).size !== widths.length
    || task.requiredViewportWidths.some((width) => !widths.includes(width))) {
    throw new Error('Annotation validation must cover every required viewport exactly once.');
  }
  if (validation.viewports.some((viewport) => !viewport.layoutPassed || !viewport.snapshotId || !viewport.calibrationPassed)) {
    throw new Error('Annotation validation requires passing layout, snapshot, and browser calibration at every viewport.');
  }
}

export function createAnnotationResolutionTransaction(
  task: AnnotationAiTask,
  document: SceneDocument,
  scopeValidation: ScopedAiTransactionValidation,
  summary: SceneTransactionSummary,
  validation: AnnotationAiValidationArtifact,
  resolvedAt = new Date().toISOString()
): SceneTransaction {
  assertScope(task.scope, document);
  if (scopeValidation.transactionId !== summary.transactionId || validation.transactionId !== summary.transactionId) {
    throw new Error('Annotation completion artifacts reference different transactions.');
  }
  if (scopeValidation.baseRevision !== task.baseRevision || summary.baseRevision !== task.baseRevision || summary.revision !== document.revision) {
    throw new Error('Annotation completion revision chain is invalid.');
  }
  if (scopeValidation.affectedNodeIds.length === 0) throw new Error('Annotation cannot resolve without an in-scope Scene change.');
  assertValidationArtifact(task, document, validation);
  if (!Number.isFinite(Date.parse(resolvedAt))) throw new Error('Annotation resolution timestamp is invalid.');
  const target = indexSceneDocument(document).get(task.targetNodeId)?.node;
  if (!target) throw new Error('Annotation target node was removed.');
  const annotationIndex = target.annotations.findIndex((candidate) => candidate.id === task.annotationId);
  if (annotationIndex < 0 || target.annotations[annotationIndex].status !== 'open') throw new Error('Annotation is no longer open.');
  const annotations = structuredClone(target.annotations);
  annotations[annotationIndex] = { ...annotations[annotationIndex], status: 'resolved', resolvedAt };
  return {
    transactionId: `resolve:${task.annotationId}:${document.revision}`,
    baseRevision: document.revision,
    author: 'system',
    reason: `Resolve validated annotation task ${task.taskId}`,
    operations: [{ op: 'update-node', nodeId: task.targetNodeId, patches: [{ path: ['annotations'], value: annotations }] }]
  };
}
