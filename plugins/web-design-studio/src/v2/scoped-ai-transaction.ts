import {
  indexSceneDocument,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneDocument,
  type SceneNode
} from './scene-schema.js';
import type { SceneTransaction } from './scene-transaction.js';

export interface ScopedAiTransactionPolicy {
  baseRevision: number;
  allowedExistingNodeIds: string[];
  targetRootNodeIds: string[];
  dependencyNodeIds?: string[];
  preserveTargetRoots?: boolean;
  forbiddenPatchRoots?: string[];
}

export interface ScopedAiTransactionValidation {
  transactionId: string;
  baseRevision: number;
  allowedExistingNodeIds: string[];
  insertedNodeIds: string[];
  affectedNodeIds: string[];
}

function nodeChildren(node: SceneNode): SceneNode[] {
  const children: SceneNode[] = [];
  if (isSceneContainer(node)) children.push(...node.children);
  if (isSceneSlotContainer(node)) children.push(...Object.values(node.slots).flat());
  return children;
}

export function collectSceneSubtreeNodeIds(document: SceneDocument, rootNodeIds: string[]): string[] {
  const index = indexSceneDocument(document);
  const result = new Set<string>();
  const visit = (node: SceneNode): void => {
    if (result.has(node.id)) return;
    result.add(node.id);
    for (const child of nodeChildren(node)) visit(child);
  };
  for (const rootNodeId of rootNodeIds) {
    const root = index.get(rootNodeId)?.node;
    if (!root) throw new Error(`Scoped AI target node not found: ${rootNodeId}`);
    visit(root);
  }
  return [...result];
}

function collectDetachedSubtreeNodeIds(root: SceneNode): string[] {
  const result: string[] = [];
  const visit = (node: SceneNode): void => {
    result.push(node.id);
    for (const child of nodeChildren(node)) visit(child);
  };
  visit(root);
  return result;
}

function assertUniqueExistingIds(document: SceneDocument, ids: string[], label: string): void {
  if (!Array.isArray(ids) || ids.length === 0 || new Set(ids).size !== ids.length) throw new Error(`${label} must be a non-empty unique node list.`);
  const index = indexSceneDocument(document);
  for (const id of ids) if (!index.has(id)) throw new Error(`${label} references missing node ${id}.`);
}

export function validateScopedAiTransaction(
  document: SceneDocument,
  transaction: SceneTransaction,
  policy: ScopedAiTransactionPolicy
): ScopedAiTransactionValidation {
  if (document.revision !== policy.baseRevision) throw new Error(`Scoped AI policy is stale. Current revision is ${document.revision}.`);
  if (transaction.author !== 'ai') throw new Error('Scoped design transactions must be authored by AI.');
  if (transaction.baseRevision !== policy.baseRevision) throw new Error(`Scoped AI transaction must use base revision ${policy.baseRevision}.`);
  if (!Array.isArray(transaction.operations) || transaction.operations.length === 0) throw new Error('Scoped AI transaction needs at least one operation.');
  assertUniqueExistingIds(document, policy.allowedExistingNodeIds, 'allowedExistingNodeIds');
  assertUniqueExistingIds(document, policy.targetRootNodeIds, 'targetRootNodeIds');
  const dependencyNodeIds = policy.dependencyNodeIds ?? [];
  if (new Set(dependencyNodeIds).size !== dependencyNodeIds.length) throw new Error('dependencyNodeIds must be unique.');
  const documentIndex = indexSceneDocument(document);
  for (const dependencyNodeId of dependencyNodeIds) if (!documentIndex.has(dependencyNodeId)) throw new Error(`dependencyNodeIds references missing node ${dependencyNodeId}.`);

  const allowed = new Set(policy.allowedExistingNodeIds);
  const known = new Set(documentIndex.keys());
  const targets = new Set(policy.targetRootNodeIds);
  const dependencies = new Set(dependencyNodeIds);
  const forbiddenPatchRoots = new Set(policy.forbiddenPatchRoots ?? ['annotations', 'aiPolicy', 'locked']);
  const insertedNodeIds: string[] = [];
  const affectedNodeIds = new Set<string>();

  for (const operation of transaction.operations) {
    if (operation.op === 'insert-page' || operation.op === 'remove-page'
      || operation.op === 'insert-variable-collection' || operation.op === 'insert-responsive-rule'
      || operation.op === 'set-responsive-node-overrides' || operation.op === 'remove-responsive-rule' || operation.op === 'rename-page') {
      throw new Error(`Scoped AI transaction cannot perform ${operation.op}.`);
    }
    if (operation.op === 'insert-node') {
      if (!allowed.has(operation.parentId)) throw new Error(`Scoped AI insertion parent ${operation.parentId} is outside the allowed node scope.`);
      const ids = collectDetachedSubtreeNodeIds(operation.node);
      if (new Set(ids).size !== ids.length) throw new Error('Scoped AI insertion contains duplicate node IDs.');
      for (const id of ids) {
        if (known.has(id)) throw new Error(`Scoped AI insertion reuses existing node ID ${id}.`);
        known.add(id);
        allowed.add(id);
        insertedNodeIds.push(id);
        affectedNodeIds.add(id);
      }
      affectedNodeIds.add(operation.parentId);
      continue;
    }
    if (operation.op === 'set-variable-collections') {
      throw new Error('Scoped AI transactions cannot replace all variable collections.');
    }
    if (!allowed.has(operation.nodeId)) throw new Error(`Scoped AI operation targets ${operation.nodeId} outside the allowed node scope.`);
    if (operation.op === 'update-node') {
      for (const patch of operation.patches) {
        if (forbiddenPatchRoots.has(patch.path[0])) throw new Error(`Scoped AI transaction cannot patch ${patch.path[0]} on ${operation.nodeId}.`);
      }
      affectedNodeIds.add(operation.nodeId);
      continue;
    }
    if (operation.op === 'remove-node') {
      if (dependencies.has(operation.nodeId)) throw new Error(`Scoped AI transaction cannot remove dependency node ${operation.nodeId}.`);
      if ((policy.preserveTargetRoots ?? true) && targets.has(operation.nodeId)) throw new Error(`Scoped AI transaction cannot remove target root ${operation.nodeId}.`);
      affectedNodeIds.add(operation.nodeId);
      continue;
    }
    if (dependencies.has(operation.nodeId)) throw new Error(`Scoped AI transaction cannot move dependency node ${operation.nodeId}.`);
    if (!allowed.has(operation.parentId)) throw new Error(`Scoped AI move destination ${operation.parentId} is outside the allowed node scope.`);
    affectedNodeIds.add(operation.nodeId);
    affectedNodeIds.add(operation.parentId);
  }

  return {
    transactionId: transaction.transactionId,
    baseRevision: transaction.baseRevision,
    allowedExistingNodeIds: [...policy.allowedExistingNodeIds],
    insertedNodeIds,
    affectedNodeIds: [...affectedNodeIds]
  };
}
