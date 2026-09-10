import { indexSceneDocument, type SceneDocument } from './scene-schema.js';
import type { SceneTransaction } from './scene-transaction.js';
import type { GenerationSoftProtectionConflict } from './generation-candidate-store.js';

export interface GenerationSoftProtectedField {
  nodeId: string;
  path: string[];
  protectedAtRevision: number;
  reason: string;
}

function pathsOverlap(left: string[], right: string[]): boolean {
  if (left[0] === '$node' || right[0] === '$node') return true;
  const length = Math.min(left.length, right.length);
  for (let index = 0; index < length; index += 1) if (left[index] !== right[index]) return false;
  return true;
}

function detachedNodeIds(root: import('./scene-schema.js').SceneNode): string[] {
  const result: string[] = [];
  const visit = (node: import('./scene-schema.js').SceneNode): void => {
    result.push(node.id);
    if ('children' in node) for (const child of node.children) visit(child);
    if ('slots' in node) for (const child of Object.values(node.slots).flat()) visit(child);
  };
  visit(root);
  return result;
}

export function softProtectionsFromHumanTransaction(
  transaction: SceneTransaction,
  committedRevision: number,
  reason = 'This field was manually edited by a human.'
): GenerationSoftProtectedField[] {
  if (transaction.author !== 'human') throw new Error('Soft protections can be recorded only from a human Scene transaction.');
  if (!Number.isSafeInteger(committedRevision) || committedRevision !== transaction.baseRevision + 1) {
    throw new Error('Soft protection committed revision must immediately follow the human transaction base revision.');
  }
  const protections: GenerationSoftProtectedField[] = [];
  const push = (nodeId: string, path: string[], detail = reason): void => {
    const key = `${nodeId}\0${path.join('.')}`;
    if (protections.some((candidate) => `${candidate.nodeId}\0${candidate.path.join('.')}` === key)) return;
    protections.push({ nodeId, path: [...path], protectedAtRevision: committedRevision, reason: detail });
  };
  for (const operation of transaction.operations) {
    if (operation.op === 'update-node') for (const patch of operation.patches) push(operation.nodeId, patch.path);
    else if (operation.op === 'move-node') push(operation.nodeId, ['parent'], 'This node was manually moved by a human.');
    else if (operation.op === 'insert-node') {
      for (const nodeId of detachedNodeIds(operation.node)) push(nodeId, ['$node'], 'This node was manually created by a human.');
    }
  }
  return protections;
}

function descendantIds(document: SceneDocument, rootId: string): string[] {
  const index = indexSceneDocument(document);
  const result: string[] = [];
  for (const [nodeId, entry] of index) {
    let current = entry;
    if (nodeId === rootId) {
      result.push(nodeId);
      continue;
    }
    while (current.parentId && !document.pages.some((page) => page.id === current.parentId)) {
      if (current.parentId === rootId) {
        result.push(nodeId);
        break;
      }
      const parent = index.get(current.parentId);
      if (!parent) break;
      current = parent;
    }
  }
  return result;
}

export function findGenerationSoftProtectionConflicts(
  document: SceneDocument,
  transaction: SceneTransaction,
  protections: GenerationSoftProtectedField[]
): GenerationSoftProtectionConflict[] {
  if (transaction.baseRevision !== document.revision) throw new Error('Soft protection check needs a transaction at the current Scene revision.');
  const index = indexSceneDocument(document);
  const unique = new Set<string>();
  for (const protection of protections) {
    if (!index.has(protection.nodeId)) throw new Error(`Soft protection references missing node ${protection.nodeId}.`);
    if (!Array.isArray(protection.path) || protection.path.length === 0 || protection.path.some((segment) => !segment)) throw new Error('Soft protection path is invalid.');
    if (!Number.isSafeInteger(protection.protectedAtRevision) || protection.protectedAtRevision < 0 || protection.protectedAtRevision > document.revision) {
      throw new Error('Soft protection revision is invalid.');
    }
    if (!protection.reason?.trim()) throw new Error('Soft protection reason is required.');
    const key = `${protection.nodeId}\0${protection.path.join('.')}`;
    if (unique.has(key)) throw new Error(`Duplicate soft protection for ${protection.nodeId}/${protection.path.join('.')}.`);
    unique.add(key);
  }

  const conflicts: GenerationSoftProtectionConflict[] = [];
  const push = (protection: GenerationSoftProtectedField, requestedPath: string[]): void => {
    if (conflicts.some((conflict) => conflict.nodeId === protection.nodeId
      && conflict.protectedPath.join('.') === protection.path.join('.')
      && conflict.requestedPath.join('.') === requestedPath.join('.'))) return;
    conflicts.push({
      nodeId: protection.nodeId,
      protectedPath: [...protection.path],
      requestedPath: [...requestedPath],
      protectedAtRevision: protection.protectedAtRevision,
      reason: protection.reason
    });
  };

  for (const operation of transaction.operations) {
    if (operation.op === 'update-node') {
      for (const patch of operation.patches) {
        for (const protection of protections.filter((candidate) => candidate.nodeId === operation.nodeId)) {
          if (pathsOverlap(protection.path, patch.path)) push(protection, patch.path);
        }
      }
      continue;
    }
    if (operation.op === 'move-node') {
      for (const protection of protections.filter((candidate) => candidate.nodeId === operation.nodeId && pathsOverlap(candidate.path, ['parent']))) {
        push(protection, ['parent']);
      }
      continue;
    }
    if (operation.op === 'remove-node') {
      const removed = new Set(descendantIds(document, operation.nodeId));
      for (const protection of protections.filter((candidate) => removed.has(candidate.nodeId))) push(protection, ['remove']);
    }
  }
  return conflicts;
}
