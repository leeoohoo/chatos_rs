import {
  assertSceneDocument,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneDocument,
  type SceneNode,
  type ScenePage,
  type SceneVariable,
  type SceneVariableCollection
} from './scene-schema.js';

export type SceneDiffEntity = 'document' | 'page' | 'node' | 'variable-collection' | 'variable';

export interface SceneDiffLocation {
  pageId: string;
  parentId: string;
  index: number;
  slot?: string;
}

export interface SceneFieldChange {
  kind: 'field-changed';
  entity: SceneDiffEntity;
  entityId: string;
  path: string[];
  before: unknown;
  after: unknown;
}

export interface SceneEntityAddedChange {
  kind: 'entity-added';
  entity: Exclude<SceneDiffEntity, 'document'>;
  entityId: string;
  value: unknown;
  location?: SceneDiffLocation;
}

export interface SceneEntityRemovedChange {
  kind: 'entity-removed';
  entity: Exclude<SceneDiffEntity, 'document'>;
  entityId: string;
  value: unknown;
  location?: SceneDiffLocation;
}

export interface SceneEntityMovedChange {
  kind: 'entity-moved';
  entity: 'page' | 'node';
  entityId: string;
  before: SceneDiffLocation | { index: number };
  after: SceneDiffLocation | { index: number };
}

export type SceneDiffChange = SceneFieldChange | SceneEntityAddedChange | SceneEntityRemovedChange | SceneEntityMovedChange;

export interface SceneDiffSummary {
  fieldsChanged: number;
  entitiesAdded: number;
  entitiesRemoved: number;
  entitiesMoved: number;
}

export interface SceneDocumentDiff {
  beforeDocumentId: string;
  afterDocumentId: string;
  beforeRevision: number;
  afterRevision: number;
  changes: SceneDiffChange[];
  summary: SceneDiffSummary;
}

interface NodeEntry {
  node: SceneNode;
  location: SceneDiffLocation;
  ancestorIds: string[];
}

interface DocumentIndex {
  nodes: Map<string, NodeEntry>;
  nodeOrder: string[];
  collections: Map<string, string[]>;
  pages: Map<string, ScenePage>;
  pageOrder: string[];
  variableCollections: Map<string, SceneVariableCollection>;
  variableCollectionOrder: string[];
  variables: Map<string, { variable: SceneVariable; collectionId: string }>;
  variableOrder: string[];
}

function collectionKey(location: Pick<SceneDiffLocation, 'pageId' | 'parentId' | 'slot'>): string {
  return `${location.pageId}\u0000${location.parentId}\u0000${location.slot ?? ''}`;
}

function indexDocument(document: SceneDocument): DocumentIndex {
  const nodes = new Map<string, NodeEntry>();
  const nodeOrder: string[] = [];
  const collections = new Map<string, string[]>();

  function visitCollection(children: SceneNode[], pageId: string, parentId: string, ancestorIds: string[], slot?: string): void {
    const key = collectionKey({ pageId, parentId, slot });
    collections.set(key, children.map((child) => child.id));
    for (const [index, node] of children.entries()) {
      const location: SceneDiffLocation = { pageId, parentId, index, ...(slot === undefined ? {} : { slot }) };
      nodes.set(node.id, { node, location, ancestorIds });
      nodeOrder.push(node.id);
      const nextAncestors = [...ancestorIds, node.id];
      if (isSceneContainer(node)) visitCollection(node.children, pageId, node.id, nextAncestors);
      if (isSceneSlotContainer(node)) {
        for (const [slotName, slotChildren] of Object.entries(node.slots)) visitCollection(slotChildren, pageId, node.id, nextAncestors, slotName);
      }
    }
  }

  for (const page of document.pages) visitCollection(page.children, page.id, page.id, []);
  const variableCollections = new Map(document.variableCollections.map((collection) => [collection.id, collection]));
  const variables = new Map<string, { variable: SceneVariable; collectionId: string }>();
  const variableOrder: string[] = [];
  for (const collection of document.variableCollections) {
    for (const variable of collection.variables) {
      variables.set(variable.id, { variable, collectionId: collection.id });
      variableOrder.push(variable.id);
    }
  }
  return {
    nodes,
    nodeOrder,
    collections,
    pages: new Map(document.pages.map((page) => [page.id, page])),
    pageOrder: document.pages.map((page) => page.id),
    variableCollections,
    variableCollectionOrder: document.variableCollections.map((collection) => collection.id),
    variables,
    variableOrder
  };
}

function cloneValue(value: unknown): unknown {
  return value === undefined ? undefined : structuredClone(value);
}

function plainNode(node: SceneNode): Record<string, unknown> {
  const copy = { ...node } as Record<string, unknown>;
  delete copy.children;
  if (isSceneSlotContainer(node)) {
    copy.slots = Object.fromEntries(Object.keys(node.slots).map((slot) => [slot, []]));
  }
  return copy;
}

function plainPage(page: ScenePage): Record<string, unknown> {
  return { id: page.id, name: page.name };
}

function plainVariableCollection(collection: SceneVariableCollection): Record<string, unknown> {
  return { id: collection.id, name: collection.name, modes: collection.modes };
}

function diffValue(
  before: unknown,
  after: unknown,
  path: string[],
  entity: SceneDiffEntity,
  entityId: string,
  changes: SceneDiffChange[]
): void {
  if (Object.is(before, after)) return;
  const beforeObject = before !== null && typeof before === 'object';
  const afterObject = after !== null && typeof after === 'object';
  if (beforeObject && afterObject && Array.isArray(before) === Array.isArray(after)) {
    if (Array.isArray(before) && Array.isArray(after)) {
      const length = Math.max(before.length, after.length);
      for (let index = 0; index < length; index += 1) diffValue(before[index], after[index], [...path, String(index)], entity, entityId, changes);
      return;
    }
    const beforeRecord = before as Record<string, unknown>;
    const afterRecord = after as Record<string, unknown>;
    const keys = [...new Set([...Object.keys(beforeRecord), ...Object.keys(afterRecord)])].sort();
    for (const key of keys) diffValue(beforeRecord[key], afterRecord[key], [...path, key], entity, entityId, changes);
    return;
  }
  changes.push({
    kind: 'field-changed',
    entity,
    entityId,
    path,
    before: cloneValue(before),
    after: cloneValue(after)
  });
}

function longestCommonSubsequence(left: string[], right: string[]): Set<string> {
  const rightPositions = new Map(right.map((id, index) => [id, index]));
  const sequence = left.flatMap((id) => rightPositions.has(id) ? [{ id, position: rightPositions.get(id)! }] : []);
  if (sequence.length === 0) return new Set();
  const tails: number[] = [];
  const previous = new Int32Array(sequence.length).fill(-1);
  for (const [sequenceIndex, item] of sequence.entries()) {
    let low = 0;
    let high = tails.length;
    while (low < high) {
      const middle = (low + high) >> 1;
      if (sequence[tails[middle]].position < item.position) low = middle + 1;
      else high = middle;
    }
    if (low > 0) previous[sequenceIndex] = tails[low - 1];
    tails[low] = sequenceIndex;
  }
  const retained = new Set<string>();
  let cursor = tails.at(-1) ?? -1;
  while (cursor >= 0) {
    retained.add(sequence[cursor].id);
    cursor = previous[cursor];
  }
  return retained;
}

function rootEntityIds(ids: string[], entries: Map<string, NodeEntry>, candidateIds: Set<string>): string[] {
  return ids.filter((id) => !entries.get(id)!.ancestorIds.some((ancestorId) => candidateIds.has(ancestorId)));
}

function sameCollection(left: SceneDiffLocation, right: SceneDiffLocation): boolean {
  return left.pageId === right.pageId && left.parentId === right.parentId && left.slot === right.slot;
}

function movedNodeIds(before: DocumentIndex, after: DocumentIndex, commonNodeIds: Set<string>): Set<string> {
  const moved = new Set<string>();
  for (const id of commonNodeIds) {
    if (!sameCollection(before.nodes.get(id)!.location, after.nodes.get(id)!.location)) moved.add(id);
  }
  const collectionKeys = new Set([...before.collections.keys(), ...after.collections.keys()]);
  for (const key of collectionKeys) {
    const left = (before.collections.get(key) ?? []).filter((id) => commonNodeIds.has(id) && sameCollection(before.nodes.get(id)!.location, after.nodes.get(id)!.location));
    const right = (after.collections.get(key) ?? []).filter((id) => commonNodeIds.has(id) && sameCollection(before.nodes.get(id)!.location, after.nodes.get(id)!.location));
    const retained = longestCommonSubsequence(left, right);
    for (const id of left) if (!retained.has(id)) moved.add(id);
  }
  return moved;
}

function pushOrderedEntityChanges<T>(
  beforeOrder: string[],
  afterOrder: string[],
  beforeEntities: Map<string, T>,
  afterEntities: Map<string, T>,
  entity: SceneEntityAddedChange['entity'],
  changes: SceneDiffChange[]
): void {
  for (const id of beforeOrder) {
    if (!afterEntities.has(id)) changes.push({ kind: 'entity-removed', entity, entityId: id, value: cloneValue(beforeEntities.get(id)) });
  }
  for (const id of afterOrder) {
    if (!beforeEntities.has(id)) changes.push({ kind: 'entity-added', entity, entityId: id, value: cloneValue(afterEntities.get(id)) });
  }
}

export function diffSceneDocuments(beforeDocument: SceneDocument, afterDocument: SceneDocument): SceneDocumentDiff {
  assertSceneDocument(beforeDocument);
  assertSceneDocument(afterDocument);
  const before = indexDocument(beforeDocument);
  const after = indexDocument(afterDocument);
  const changes: SceneDiffChange[] = [];

  diffValue(
    {
      schemaVersion: beforeDocument.schemaVersion,
      documentId: beforeDocument.documentId,
      revision: beforeDocument.revision,
      name: beforeDocument.name,
      createdAt: beforeDocument.createdAt,
      updatedAt: beforeDocument.updatedAt
    },
    {
      schemaVersion: afterDocument.schemaVersion,
      documentId: afterDocument.documentId,
      revision: afterDocument.revision,
      name: afterDocument.name,
      createdAt: afterDocument.createdAt,
      updatedAt: afterDocument.updatedAt
    },
    [],
    'document',
    beforeDocument.documentId,
    changes
  );

  const removedPageIds = before.pageOrder.filter((id) => !after.pages.has(id));
  const addedPageIds = after.pageOrder.filter((id) => !before.pages.has(id));
  for (const id of removedPageIds) changes.push({ kind: 'entity-removed', entity: 'page', entityId: id, value: cloneValue(before.pages.get(id)) });
  for (const id of addedPageIds) changes.push({ kind: 'entity-added', entity: 'page', entityId: id, value: cloneValue(after.pages.get(id)) });
  const commonPageIds = new Set(before.pageOrder.filter((id) => after.pages.has(id)));
  const retainedPages = longestCommonSubsequence(before.pageOrder.filter((id) => commonPageIds.has(id)), after.pageOrder.filter((id) => commonPageIds.has(id)));
  for (const id of before.pageOrder) {
    if (commonPageIds.has(id) && !retainedPages.has(id)) {
      changes.push({
        kind: 'entity-moved',
        entity: 'page',
        entityId: id,
        before: { index: before.pageOrder.indexOf(id) },
        after: { index: after.pageOrder.indexOf(id) }
      });
    }
  }
  for (const id of before.pageOrder) {
    if (commonPageIds.has(id)) diffValue(plainPage(before.pages.get(id)!), plainPage(after.pages.get(id)!), [], 'page', id, changes);
  }

  const removedNodeIds = new Set(before.nodeOrder.filter((id) => !after.nodes.has(id) && commonPageIds.has(before.nodes.get(id)!.location.pageId)));
  const addedNodeIds = new Set(after.nodeOrder.filter((id) => !before.nodes.has(id) && commonPageIds.has(after.nodes.get(id)!.location.pageId)));
  for (const id of rootEntityIds(before.nodeOrder, before.nodes, removedNodeIds)) {
    if (!removedNodeIds.has(id)) continue;
    const entry = before.nodes.get(id)!;
    changes.push({ kind: 'entity-removed', entity: 'node', entityId: id, value: cloneValue(entry.node), location: cloneValue(entry.location) as SceneDiffLocation });
  }
  for (const id of rootEntityIds(after.nodeOrder, after.nodes, addedNodeIds)) {
    if (!addedNodeIds.has(id)) continue;
    const entry = after.nodes.get(id)!;
    changes.push({ kind: 'entity-added', entity: 'node', entityId: id, value: cloneValue(entry.node), location: cloneValue(entry.location) as SceneDiffLocation });
  }
  const commonNodeIds = new Set(before.nodeOrder.filter((id) => after.nodes.has(id)));
  const moved = movedNodeIds(before, after, commonNodeIds);
  for (const id of before.nodeOrder) {
    if (!moved.has(id)) continue;
    changes.push({
      kind: 'entity-moved',
      entity: 'node',
      entityId: id,
      before: cloneValue(before.nodes.get(id)!.location) as SceneDiffLocation,
      after: cloneValue(after.nodes.get(id)!.location) as SceneDiffLocation
    });
  }
  for (const id of before.nodeOrder) {
    if (commonNodeIds.has(id)) diffValue(plainNode(before.nodes.get(id)!.node), plainNode(after.nodes.get(id)!.node), [], 'node', id, changes);
  }

  pushOrderedEntityChanges(
    before.variableCollectionOrder,
    after.variableCollectionOrder,
    before.variableCollections,
    after.variableCollections,
    'variable-collection',
    changes
  );
  const commonCollectionIds = before.variableCollectionOrder.filter((id) => after.variableCollections.has(id));
  for (const id of commonCollectionIds) {
    diffValue(plainVariableCollection(before.variableCollections.get(id)!), plainVariableCollection(after.variableCollections.get(id)!), [], 'variable-collection', id, changes);
  }
  const beforeVariableValues = new Map([...before.variables].map(([id, entry]) => [id, entry.variable]));
  const afterVariableValues = new Map([...after.variables].map(([id, entry]) => [id, entry.variable]));
  pushOrderedEntityChanges(before.variableOrder, after.variableOrder, beforeVariableValues, afterVariableValues, 'variable', changes);
  for (const id of before.variableOrder) {
    if (!after.variables.has(id)) continue;
    const beforeEntry = before.variables.get(id)!;
    const afterEntry = after.variables.get(id)!;
    if (beforeEntry.collectionId !== afterEntry.collectionId) {
      changes.push({
        kind: 'field-changed',
        entity: 'variable',
        entityId: id,
        path: ['collectionId'],
        before: beforeEntry.collectionId,
        after: afterEntry.collectionId
      });
    }
    diffValue(beforeEntry.variable, afterEntry.variable, [], 'variable', id, changes);
  }

  return {
    beforeDocumentId: beforeDocument.documentId,
    afterDocumentId: afterDocument.documentId,
    beforeRevision: beforeDocument.revision,
    afterRevision: afterDocument.revision,
    changes,
    summary: {
      fieldsChanged: changes.filter((change) => change.kind === 'field-changed').length,
      entitiesAdded: changes.filter((change) => change.kind === 'entity-added').length,
      entitiesRemoved: changes.filter((change) => change.kind === 'entity-removed').length,
      entitiesMoved: changes.filter((change) => change.kind === 'entity-moved').length
    }
  };
}
