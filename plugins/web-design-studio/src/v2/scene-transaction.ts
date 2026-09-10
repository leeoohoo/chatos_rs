import {
  assertSceneDocument,
  indexSceneDocument,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneCreator,
  type SceneDocument,
  type SceneNode,
  type ScenePage,
  type SceneResponsiveRule,
  type SceneVariableCollection
} from './scene-schema.js';

export type SceneFieldPath = string[];

export type SceneTransactionOperation =
  | { op: 'insert-page'; index: number; page: ScenePage }
  | { op: 'remove-page'; pageId: string }
  | { op: 'insert-node'; parentId: string; index: number; node: SceneNode; slot?: string }
  | { op: 'update-node'; nodeId: string; patches: Array<{ path: SceneFieldPath; value: unknown }> }
  | { op: 'remove-node'; nodeId: string }
  | { op: 'move-node'; nodeId: string; parentId: string; index: number; slot?: string }
  | { op: 'insert-variable-collection'; index: number; collection: SceneVariableCollection }
  | { op: 'set-variable-collections'; collections: SceneVariableCollection[] }
  | { op: 'insert-responsive-rule'; index: number; rule: SceneResponsiveRule }
  | { op: 'remove-responsive-rule'; ruleId: string }
  | { op: 'set-responsive-node-overrides'; ruleId: string; nodeOverrides: SceneResponsiveRule['nodeOverrides'] }
  | { op: 'rename-page'; pageId: string; name: string };

export interface SceneTransaction {
  transactionId: string;
  baseRevision: number;
  author: SceneCreator;
  reason?: string;
  metadata?: Record<string, string | number | boolean>;
  operations: SceneTransactionOperation[];
}

export interface SceneTransactionSummary {
  transactionId: string;
  baseRevision: number;
  revision: number;
  insertedPageIds: string[];
  removedPageIds: string[];
  insertedNodeIds: string[];
  updatedNodeIds: string[];
  removedNodeIds: string[];
  movedNodeIds: string[];
  insertedVariableCollectionIds: string[];
  updatedVariableCollectionIds: string[];
  removedVariableCollectionIds: string[];
  insertedResponsiveRuleIds: string[];
  removedResponsiveRuleIds: string[];
  updatedResponsiveRuleIds: string[];
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
  const property = path[path.length - 1];
  if (path.length === 1 && property === 'prototypeLink' && value === null) delete cursor[property];
  else cursor[property] = structuredClone(value);
}

function clearPrototypeLinksToPages(document: SceneDocument, pageIds: ReadonlySet<string>, author: SceneCreator, timestamp: string): string[] {
  const updated: string[] = [];
  for (const entry of indexSceneDocument(document).values()) {
    if (!entry.node.prototypeLink || !pageIds.has(entry.node.prototypeLink.targetPageId)) continue;
    delete entry.node.prototypeLink;
    entry.node.updatedBy = author;
    entry.node.updatedAt = timestamp;
    updated.push(entry.node.id);
  }
  return updated;
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
    if (!slot) throw new Error(`Scene instance ${parent.id} requires a slot.`);
    if (!Object.hasOwn(parent.slots, slot)) {
      if (parent.type !== 'library-instance') throw new Error(`Scene instance ${parent.id} requires an existing slot.`);
      parent.slots[slot] = [];
    }
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
    insertedPageIds: [],
    removedPageIds: [],
    insertedNodeIds: [],
    updatedNodeIds: [],
    removedNodeIds: [],
    movedNodeIds: [],
    insertedVariableCollectionIds: [],
    updatedVariableCollectionIds: [],
    removedVariableCollectionIds: [],
    insertedResponsiveRuleIds: [],
    removedResponsiveRuleIds: [],
    updatedResponsiveRuleIds: [],
    renamedPageIds: []
  };

  for (const operation of transaction.operations) {
    if (operation.op === 'insert-page') {
      if (transaction.author === 'ai') throw new Error('AI generation transactions cannot insert pages.');
      assertIndex(operation.index, document.pages.length);
      if (document.pages.some((page) => page.id === operation.page.id) || indexSceneDocument(document).has(operation.page.id)
        || operation.page.id === document.documentId) throw new Error(`Duplicate scene id: ${operation.page.id}`);
      const page = structuredClone(operation.page);
      if (!page.name.trim()) throw new Error('Scene page name cannot be empty.');
      const insertedIds = page.children.flatMap((node) => descendantIds(node));
      for (const id of insertedIds) if (document.pages.some((candidate) => candidate.id === id) || indexSceneDocument(document).has(id)
        || id === document.documentId || id === page.id) throw new Error(`Duplicate scene id: ${id}`);
      for (const node of page.children) stampInsertedNode(node, transaction.author, timestamp);
      document.pages.splice(operation.index, 0, page);
      summary.insertedPageIds.push(page.id);
      summary.insertedNodeIds.push(...insertedIds);
      continue;
    }
    if (operation.op === 'remove-page') {
      if (transaction.author === 'ai') throw new Error('AI generation transactions cannot remove pages.');
      if (document.pages.length <= 1) throw new Error('A Scene document must keep at least one page.');
      const pageIndex = document.pages.findIndex((page) => page.id === operation.pageId);
      if (pageIndex < 0) throw new Error(`Scene page not found: ${operation.pageId}`);
      summary.updatedNodeIds.push(...clearPrototypeLinksToPages(document, new Set([operation.pageId]), transaction.author, timestamp));
      const [page] = document.pages.splice(pageIndex, 1);
      const removedIds = new Set(page.children.flatMap((node) => descendantIds(node)));
      for (const rule of document.responsiveRules) {
        const nextOverrides = rule.nodeOverrides.filter((override) => !removedIds.has(override.nodeId));
        if (nextOverrides.length !== rule.nodeOverrides.length) {
          rule.nodeOverrides = nextOverrides;
          summary.updatedResponsiveRuleIds.push(rule.id);
        }
      }
      summary.removedPageIds.push(page.id);
      summary.removedNodeIds.push(...removedIds);
      continue;
    }
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
    if (operation.op === 'set-variable-collections') {
      if (transaction.author === 'ai') throw new Error('AI generation transactions cannot replace variable collections wholesale.');
      const previousIds = new Set(document.variableCollections.map((collection) => collection.id));
      const nextIds = new Set(operation.collections.map((collection) => collection.id));
      document.variableCollections = structuredClone(operation.collections);
      summary.insertedVariableCollectionIds.push(...[...nextIds].filter((id) => !previousIds.has(id)));
      summary.updatedVariableCollectionIds.push(...[...nextIds].filter((id) => previousIds.has(id)));
      summary.removedVariableCollectionIds.push(...[...previousIds].filter((id) => !nextIds.has(id)));
      continue;
    }
    if (operation.op === 'insert-responsive-rule') {
      assertIndex(operation.index, document.responsiveRules.length);
      document.responsiveRules.splice(operation.index, 0, structuredClone(operation.rule));
      summary.insertedResponsiveRuleIds.push(operation.rule.id);
      continue;
    }
    if (operation.op === 'remove-responsive-rule') {
      if (transaction.author === 'ai') throw new Error('AI generation transactions cannot remove responsive rules.');
      const ruleIndex = document.responsiveRules.findIndex((candidate) => candidate.id === operation.ruleId);
      if (ruleIndex < 0) throw new Error(`Scene responsive rule not found: ${operation.ruleId}`);
      const [rule] = document.responsiveRules.splice(ruleIndex, 1);
      summary.removedResponsiveRuleIds.push(rule.id);
      continue;
    }
    if (operation.op === 'set-responsive-node-overrides') {
      if (transaction.author === 'ai') throw new Error('AI editor transactions cannot replace responsive rule overrides directly.');
      const rule = document.responsiveRules.find((candidate) => candidate.id === operation.ruleId);
      if (!rule) throw new Error(`Scene responsive rule not found: ${operation.ruleId}`);
      rule.nodeOverrides = structuredClone(operation.nodeOverrides);
      summary.updatedResponsiveRuleIds.push(rule.id);
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
