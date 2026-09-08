import {
  assertSceneDocument,
  indexSceneDocument,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneCreator,
  type SceneDocument,
  type SceneNode,
  type SceneResponsiveRule,
  type SceneVariableCollection
} from './scene-schema.js';

export type SceneFieldPath = string[];

export type SceneTransactionOperation =
  | { op: 'insert-node'; parentId: string; index: number; node: SceneNode; slot?: string }
  | { op: 'update-node'; nodeId: string; patches: Array<{ path: SceneFieldPath; value: unknown }> }
  | { op: 'remove-node'; nodeId: string }
  | { op: 'move-node'; nodeId: string; parentId: string; index: number; slot?: string }
  | { op: 'insert-variable-collection'; index: number; collection: SceneVariableCollection }
  | { op: 'insert-responsive-rule'; index: number; rule: SceneResponsiveRule }
  | { op: 'rename-page'; pageId: string; name: string };

export interface SceneTransaction {
  transactionId: string;
  baseRevision: number;
  author: SceneCreator;
  reason?: string;
  operations: SceneTransactionOperation[];
}

export interface SceneTransactionSummary {
  transactionId: string;
  baseRevision: number;
  revision: number;
  insertedNodeIds: string[];
  updatedNodeIds: string[];
  removedNodeIds: string[];
  movedNodeIds: string[];
  insertedVariableCollectionIds: string[];
  insertedResponsiveRuleIds: string[];
  renamedPageIds: string[];
}

export interface SceneTransactionResult {
  document: SceneDocument;
  summary: SceneTransactionSummary;
}

type MutableCollection = SceneNode[];

type NodeLocation = {
  node: SceneNode;
  parentId: string;
  collection: MutableCollection;
  index: number;
  slot?: string;
};

const protectedRoots = new Set(['id', 'type', 'children', 'slots', 'createdAt', 'createdBy', 'updatedAt', 'updatedBy']);
const dangerousPathSegments = new Set(['__proto__', 'prototype', 'constructor']);

function isAi(author: SceneCreator): boolean {
  return author === 'ai';
}

function pathsOverlap(left: string[], right: string[]): boolean {
  const sharedLength = Math.min(left.length, right.length);
  for (let index = 0; index < sharedLength; index += 1) if (left[index] !== right[index]) return false;
  return true;
}

function assertPatchAllowed(node: SceneNode, path: string[], author: SceneCreator): void {
  if (path.length === 0 || path.some((segment) => !segment || dangerousPathSegments.has(segment))) throw new Error('Scene patch path is invalid.');
  if (protectedRoots.has(path[0])) throw new Error(`Scene patch cannot update ${path[0]}.`);
  if (!isAi(author)) return;
  if (node.locked || !node.aiPolicy.editable) throw new Error(`AI cannot edit locked node ${node.id}.`);
  if (path[0] === 'locked' || path[0] === 'aiPolicy') throw new Error(`AI cannot change protection policy on ${node.id}.`);
  const locked = node.aiPolicy.lockedFields.map((field) => field.split('.'));
  if (locked.some((lockedPath) => pathsOverlap(path, lockedPath))) {
    throw new Error(`AI cannot edit human-locked field ${path.join('.')} on ${node.id}.`);
  }
}

function setAtPath(target: Record<string, unknown>, path: string[], value: unknown): void {
  let cursor = target;
  for (const segment of path.slice(0, -1)) {
    const next = cursor[segment];
    if (!next || typeof next !== 'object' || Array.isArray(next)) throw new Error(`Scene patch parent ${segment} is not an object.`);
    cursor = next as Record<string, unknown>;
  }
  cursor[path[path.length - 1]] = structuredClone(value);
}

function childCollections(node: SceneNode): Array<{ collection: MutableCollection; slot?: string }> {
  const collections: Array<{ collection: MutableCollection; slot?: string }> = [];
  if (isSceneContainer(node)) collections.push({ collection: node.children as MutableCollection });
  if (isSceneSlotContainer(node)) {
    for (const [slot, children] of Object.entries(node.slots)) collections.push({ collection: children, slot });
  }
  return collections;
}

function findNodeLocation(document: SceneDocument, nodeId: string): NodeLocation | undefined {
  function search(collection: MutableCollection, parentId: string, slot?: string): NodeLocation | undefined {
    for (const [index, node] of collection.entries()) {
      if (node.id === nodeId) return { node, parentId, collection, index, slot };
      for (const children of childCollections(node)) {
        const found = search(children.collection, node.id, children.slot);
        if (found) return found;
      }
    }
    return undefined;
  }
  for (const page of document.pages) {
    const found = search(page.children as MutableCollection, page.id);
    if (found) return found;
  }
  return undefined;
}

function collectionForParent(document: SceneDocument, parentId: string, slot?: string): MutableCollection {
  const page = document.pages.find((candidate) => candidate.id === parentId);
  if (page) {
    if (slot !== undefined) throw new Error('Page children do not use slots.');
    return page.children as MutableCollection;
  }
  const location = findNodeLocation(document, parentId);
  if (!location) throw new Error(`Scene parent not found: ${parentId}`);
  const parent = location.node;
  if (isSceneSlotContainer(parent)) {
    if (!slot || !Object.hasOwn(parent.slots, slot)) throw new Error(`Scene instance ${parent.id} requires an existing slot.`);
    return parent.slots[slot];
  }
  if (slot !== undefined) throw new Error(`Scene parent ${parent.id} does not accept a slot.`);
  if (!isSceneContainer(parent)) throw new Error(`Scene parent ${parent.id} cannot contain children.`);
  return parent.children as MutableCollection;
}

function descendantIds(node: SceneNode): string[] {
  const ids = [node.id];
  for (const children of childCollections(node)) for (const child of children.collection) ids.push(...descendantIds(child));
  return ids;
}

function subtreeNodes(node: SceneNode): SceneNode[] {
  const nodes = [node];
  for (const children of childCollections(node)) for (const child of children.collection) nodes.push(...subtreeNodes(child));
  return nodes;
}

function assertAiCanReparent(node: SceneNode, author: SceneCreator): void {
  if (!isAi(author)) return;
  if (subtreeNodes(node).some((candidate) => candidate.locked || !candidate.aiPolicy.editable)
    || node.aiPolicy.lockedFields.some((field) => pathsOverlap(['parent'], field.split('.')))) {
    throw new Error(`AI cannot move locked node ${node.id}.`);
  }
}

function assertAiCanRemove(node: SceneNode, author: SceneCreator): void {
  if (!isAi(author)) return;
  if (subtreeNodes(node).some((candidate) => candidate.locked || !candidate.aiPolicy.editable || candidate.aiPolicy.lockedFields.length > 0)) {
    throw new Error(`AI cannot remove protected subtree ${node.id}.`);
  }
}

function stampInsertedNode(node: SceneNode, author: SceneCreator, timestamp: string): void {
  node.createdBy = author;
  node.updatedBy = author;
  node.createdAt = timestamp;
  node.updatedAt = timestamp;
  for (const children of childCollections(node)) for (const child of children.collection) stampInsertedNode(child, author, timestamp);
}

function assertIndex(index: number, length: number): void {
  if (!Number.isSafeInteger(index) || index < 0 || index > length) throw new Error(`Scene insertion index ${index} is invalid for length ${length}.`);
}

export function applySceneTransaction(source: SceneDocument, transaction: SceneTransaction, timestamp = new Date().toISOString()): SceneTransactionResult {
  assertSceneDocument(source);
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/.test(transaction.transactionId)) throw new Error('Scene transaction id is invalid.');
  if (transaction.baseRevision !== source.revision) throw new Error(`Scene revision conflict. Current revision is ${source.revision}.`);
  if (!Array.isArray(transaction.operations) || transaction.operations.length === 0) throw new Error('Scene transaction needs at least one operation.');
  if (!Number.isFinite(Date.parse(timestamp))) throw new Error('Scene transaction timestamp is invalid.');

  const document = structuredClone(source);
  const summary: SceneTransactionSummary = {
    transactionId: transaction.transactionId,
    baseRevision: source.revision,
    revision: source.revision + 1,
    insertedNodeIds: [],
    updatedNodeIds: [],
    removedNodeIds: [],
    movedNodeIds: [],
    insertedVariableCollectionIds: [],
    insertedResponsiveRuleIds: [],
    renamedPageIds: []
  };

  for (const operation of transaction.operations) {
    if (operation.op === 'insert-node') {
      const existing = indexSceneDocument(document);
      const insertedIds = descendantIds(operation.node);
      for (const id of insertedIds) if (existing.has(id) || document.pages.some((page) => page.id === id) || id === document.documentId) throw new Error(`Duplicate scene id: ${id}`);
      const collection = collectionForParent(document, operation.parentId, operation.slot);
      assertIndex(operation.index, collection.length);
      const node = structuredClone(operation.node);
      stampInsertedNode(node, transaction.author, timestamp);
      collection.splice(operation.index, 0, node);
      summary.insertedNodeIds.push(...insertedIds);
      continue;
    }
    if (operation.op === 'update-node') {
      const location = findNodeLocation(document, operation.nodeId);
      if (!location) throw new Error(`Scene node not found: ${operation.nodeId}`);
      if (operation.patches.length === 0) throw new Error(`Scene update ${operation.nodeId} needs at least one patch.`);
      for (const patch of operation.patches) {
        assertPatchAllowed(location.node, patch.path, transaction.author);
        setAtPath(location.node as unknown as Record<string, unknown>, patch.path, patch.value);
      }
      location.node.updatedBy = transaction.author;
      location.node.updatedAt = timestamp;
      summary.updatedNodeIds.push(location.node.id);
      continue;
    }
    if (operation.op === 'remove-node') {
      const location = findNodeLocation(document, operation.nodeId);
      if (!location) throw new Error(`Scene node not found: ${operation.nodeId}`);
      assertAiCanRemove(location.node, transaction.author);
      const ids = descendantIds(location.node);
      location.collection.splice(location.index, 1);
      summary.removedNodeIds.push(...ids);
      continue;
    }
    if (operation.op === 'move-node') {
      const location = findNodeLocation(document, operation.nodeId);
      if (!location) throw new Error(`Scene node not found: ${operation.nodeId}`);
      assertAiCanReparent(location.node, transaction.author);
      if (descendantIds(location.node).includes(operation.parentId)) throw new Error(`Scene node ${operation.nodeId} cannot move into its own subtree.`);
      const node = location.node;
      location.collection.splice(location.index, 1);
      const destination = collectionForParent(document, operation.parentId, operation.slot);
      assertIndex(operation.index, destination.length);
      destination.splice(operation.index, 0, node);
      node.updatedBy = transaction.author;
      node.updatedAt = timestamp;
      summary.movedNodeIds.push(node.id);
      continue;
    }
    if (operation.op === 'insert-variable-collection') {
      assertIndex(operation.index, document.variableCollections.length);
      document.variableCollections.splice(operation.index, 0, structuredClone(operation.collection));
      summary.insertedVariableCollectionIds.push(operation.collection.id);
      continue;
    }
    if (operation.op === 'insert-responsive-rule') {
      assertIndex(operation.index, document.responsiveRules.length);
      document.responsiveRules.splice(operation.index, 0, structuredClone(operation.rule));
      summary.insertedResponsiveRuleIds.push(operation.rule.id);
      continue;
    }
    const page = document.pages.find((candidate) => candidate.id === operation.pageId);
    if (!page) throw new Error(`Scene page not found: ${operation.pageId}`);
    if (!operation.name.trim()) throw new Error('Scene page name cannot be empty.');
    page.name = operation.name.trim();
    summary.renamedPageIds.push(page.id);
  }

  document.revision = source.revision + 1;
  document.updatedAt = timestamp;
  assertSceneDocument(document);
  return { document, summary };
}
