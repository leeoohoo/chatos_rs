import {
  assertSceneDocument,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneAnnotation,
  type SceneCreator,
  type SceneDocument,
  type SceneNode,
  type SceneNodeType
} from './scene-schema.js';

export interface SceneStringMatcher {
  equals?: string;
  contains?: string;
  startsWith?: string;
  caseSensitive?: boolean;
}

export interface SceneAncestorMatcher {
  ids?: string[];
  types?: SceneNodeType[];
  roles?: string[];
  name?: SceneStringMatcher;
  direct?: boolean;
}

export interface SceneAnnotationMatcher {
  statuses?: Array<SceneAnnotation['status']>;
  authors?: SceneCreator[];
  body?: SceneStringMatcher;
}

export interface SceneLibraryBindingMatcher {
  libraries?: string[];
  components?: string[];
  variants?: string[];
}

export interface SceneComponentBindingMatcher {
  mainComponentIds?: string[];
}

export interface SceneVariableBindingMatcher {
  variableIds?: string[];
  collectionIds?: string[];
  propertyPaths?: string[];
  match?: 'any' | 'all';
}

export interface SceneQuery {
  ids?: string[];
  pageIds?: string[];
  types?: SceneNodeType[];
  roles?: string[];
  name?: SceneStringMatcher;
  ancestor?: SceneAncestorMatcher;
  createdBy?: SceneCreator[];
  updatedBy?: SceneCreator[];
  visible?: boolean;
  locked?: boolean;
  aiEditable?: boolean;
  hasLockedFields?: boolean;
  annotation?: SceneAnnotationMatcher;
  libraryBinding?: SceneLibraryBindingMatcher;
  componentBinding?: SceneComponentBindingMatcher;
  variableBinding?: SceneVariableBindingMatcher;
  limit?: number;
}

export interface SceneQueryAncestor {
  id: string;
  type: SceneNodeType;
  name: string;
  role?: string;
}

export interface SceneQueryResult {
  node: SceneNode;
  nodeId: string;
  parentId: string;
  pageId: string;
  path: number[];
  depth: number;
  slot?: string;
  ancestors: SceneQueryAncestor[];
}

interface IndexedQueryEntry {
  node: SceneNode;
  parentId: string;
  pageId: string;
  path: number[];
  slot?: string;
  ancestors: SceneNode[];
}

const sceneNodeTypes: SceneNodeType[] = [
  'section',
  'frame',
  'group',
  'text',
  'shape',
  'media',
  'library-instance',
  'component-main',
  'component-set',
  'component-instance'
];
const sceneNodeTypeSet = new Set<string>(sceneNodeTypes);

function assertNonEmptyStringList(values: string[] | undefined, label: string): void {
  if (values === undefined) return;
  if (!Array.isArray(values) || values.length === 0 || values.some((value) => typeof value !== 'string' || !value.trim())) {
    throw new Error(`${label} must be a non-empty string list.`);
  }
}

function assertCreatorList(values: SceneCreator[] | undefined, label: string): void {
  assertNonEmptyStringList(values, label);
  if (values?.some((value) => value !== 'human' && value !== 'ai' && value !== 'system' && !value.startsWith('integration:'))) {
    throw new Error(`${label} contains an invalid creator.`);
  }
}

function assertStringMatcher(matcher: SceneStringMatcher | undefined, label: string): void {
  if (matcher === undefined) return;
  if (!matcher || typeof matcher !== 'object') throw new Error(`${label} is invalid.`);
  const criteria = [matcher.equals, matcher.contains, matcher.startsWith].filter((value) => value !== undefined);
  if (criteria.length !== 1 || criteria.some((value) => typeof value !== 'string' || !value.length)) {
    throw new Error(`${label} needs exactly one non-empty matching rule.`);
  }
  if (matcher.caseSensitive !== undefined && typeof matcher.caseSensitive !== 'boolean') throw new Error(`${label}.caseSensitive is invalid.`);
}

function assertQuery(query: SceneQuery): void {
  if (!query || typeof query !== 'object') throw new Error('Scene query must be an object.');
  for (const [values, label] of [
    [query.ids, 'query.ids'],
    [query.pageIds, 'query.pageIds'],
    [query.roles, 'query.roles']
  ] as const) assertNonEmptyStringList(values, label);
  if (query.types !== undefined) {
    assertNonEmptyStringList(query.types, 'query.types');
    if (query.types.some((type) => !sceneNodeTypeSet.has(type))) throw new Error('query.types contains an invalid scene node type.');
  }
  assertStringMatcher(query.name, 'query.name');
  assertCreatorList(query.createdBy, 'query.createdBy');
  assertCreatorList(query.updatedBy, 'query.updatedBy');
  for (const [value, label] of [
    [query.visible, 'query.visible'],
    [query.locked, 'query.locked'],
    [query.aiEditable, 'query.aiEditable'],
    [query.hasLockedFields, 'query.hasLockedFields']
  ] as const) if (value !== undefined && typeof value !== 'boolean') throw new Error(`${label} is invalid.`);
  if (query.limit !== undefined && (!Number.isSafeInteger(query.limit) || query.limit <= 0)) throw new Error('query.limit must be a positive safe integer.');

  if (query.ancestor) {
    assertNonEmptyStringList(query.ancestor.ids, 'query.ancestor.ids');
    assertNonEmptyStringList(query.ancestor.roles, 'query.ancestor.roles');
    if (query.ancestor.types !== undefined) {
      assertNonEmptyStringList(query.ancestor.types, 'query.ancestor.types');
      if (query.ancestor.types.some((type) => !sceneNodeTypeSet.has(type))) throw new Error('query.ancestor.types contains an invalid scene node type.');
    }
    assertStringMatcher(query.ancestor.name, 'query.ancestor.name');
    if (query.ancestor.direct !== undefined && typeof query.ancestor.direct !== 'boolean') throw new Error('query.ancestor.direct is invalid.');
  }
  if (query.annotation) {
    if (query.annotation.statuses !== undefined) {
      assertNonEmptyStringList(query.annotation.statuses, 'query.annotation.statuses');
      if (query.annotation.statuses.some((status) => status !== 'open' && status !== 'resolved')) throw new Error('query.annotation.statuses contains an invalid status.');
    }
    assertCreatorList(query.annotation.authors, 'query.annotation.authors');
    assertStringMatcher(query.annotation.body, 'query.annotation.body');
  }
  if (query.libraryBinding) {
    assertNonEmptyStringList(query.libraryBinding.libraries, 'query.libraryBinding.libraries');
    assertNonEmptyStringList(query.libraryBinding.components, 'query.libraryBinding.components');
    assertNonEmptyStringList(query.libraryBinding.variants, 'query.libraryBinding.variants');
  }
  if (query.componentBinding) assertNonEmptyStringList(query.componentBinding.mainComponentIds, 'query.componentBinding.mainComponentIds');
  if (query.variableBinding) {
    assertNonEmptyStringList(query.variableBinding.variableIds, 'query.variableBinding.variableIds');
    assertNonEmptyStringList(query.variableBinding.collectionIds, 'query.variableBinding.collectionIds');
    assertNonEmptyStringList(query.variableBinding.propertyPaths, 'query.variableBinding.propertyPaths');
    if (query.variableBinding.match !== undefined && query.variableBinding.match !== 'any' && query.variableBinding.match !== 'all') {
      throw new Error('query.variableBinding.match is invalid.');
    }
  }
}

function includes(values: string[] | undefined, value: string | undefined): boolean {
  return values === undefined || (value !== undefined && values.includes(value));
}

function matchesString(value: string, matcher: SceneStringMatcher | undefined): boolean {
  if (!matcher) return true;
  const normalize = (candidate: string) => matcher.caseSensitive ? candidate : candidate.toLocaleLowerCase();
  const actual = normalize(value);
  if (matcher.equals !== undefined) return actual === normalize(matcher.equals);
  if (matcher.contains !== undefined) return actual.includes(normalize(matcher.contains));
  return actual.startsWith(normalize(matcher.startsWith!));
}

function matchesAncestor(node: SceneNode, matcher: SceneAncestorMatcher): boolean {
  return includes(matcher.ids, node.id)
    && includes(matcher.types, node.type)
    && includes(matcher.roles, node.role)
    && matchesString(node.name, matcher.name);
}

function matchesAnnotation(annotation: SceneAnnotation, matcher: SceneAnnotationMatcher): boolean {
  return includes(matcher.statuses, annotation.status)
    && includes(matcher.authors, annotation.author)
    && matchesString(annotation.body, matcher.body);
}

export class SceneQueryIndex {
  readonly documentId: string;
  readonly revision: number;
  private readonly entries: IndexedQueryEntry[];
  private readonly variableCollectionsByVariableId = new Map<string, string>();

  constructor(document: SceneDocument) {
    assertSceneDocument(document);
    this.documentId = document.documentId;
    this.revision = document.revision;
    for (const collection of document.variableCollections) {
      for (const variable of collection.variables) this.variableCollectionsByVariableId.set(variable.id, collection.id);
    }
    this.entries = [];
    for (const [pageIndex, page] of document.pages.entries()) {
      for (const [childIndex, child] of page.children.entries()) {
        this.visit(child, page.id, page.id, [pageIndex, childIndex], []);
      }
    }
  }

  private visit(node: SceneNode, parentId: string, pageId: string, path: number[], ancestors: SceneNode[], slot?: string): void {
    this.entries.push({ node, parentId, pageId, path, ancestors, slot });
    const nextAncestors = [...ancestors, node];
    if (isSceneContainer(node)) {
      for (const [childIndex, child] of node.children.entries()) this.visit(child, node.id, pageId, [...path, childIndex], nextAncestors);
    }
    if (isSceneSlotContainer(node)) {
      for (const [slotName, children] of Object.entries(node.slots)) {
        for (const [childIndex, child] of children.entries()) this.visit(child, node.id, pageId, [...path, childIndex], nextAncestors, slotName);
      }
    }
  }

  query(query: SceneQuery = {}): SceneQueryResult[] {
    assertQuery(query);
    const results: SceneQueryResult[] = [];
    for (const entry of this.entries) {
      if (!this.matchesEntry(entry, query)) continue;
      results.push({
        node: structuredClone(entry.node),
        nodeId: entry.node.id,
        parentId: entry.parentId,
        pageId: entry.pageId,
        path: [...entry.path],
        depth: entry.ancestors.length,
        ...(entry.slot === undefined ? {} : { slot: entry.slot }),
        ancestors: entry.ancestors.map((ancestor) => ({
          id: ancestor.id,
          type: ancestor.type,
          name: ancestor.name,
          ...(ancestor.role === undefined ? {} : { role: ancestor.role })
        }))
      });
      if (query.limit !== undefined && results.length >= query.limit) break;
    }
    return results;
  }

  private matchesEntry(entry: IndexedQueryEntry, query: SceneQuery): boolean {
    const node = entry.node;
    if (!includes(query.ids, node.id)
      || !includes(query.pageIds, entry.pageId)
      || !includes(query.types, node.type)
      || !includes(query.roles, node.role)
      || !matchesString(node.name, query.name)
      || !includes(query.createdBy, node.createdBy)
      || !includes(query.updatedBy, node.updatedBy)) return false;
    if (query.visible !== undefined && node.visible !== query.visible) return false;
    if (query.locked !== undefined && node.locked !== query.locked) return false;
    if (query.aiEditable !== undefined && (!node.locked && node.aiPolicy.editable) !== query.aiEditable) return false;
    if (query.hasLockedFields !== undefined && (node.aiPolicy.lockedFields.length > 0) !== query.hasLockedFields) return false;
    if (query.ancestor) {
      const candidates = query.ancestor.direct ? entry.ancestors.slice(-1) : entry.ancestors;
      if (!candidates.some((ancestor) => matchesAncestor(ancestor, query.ancestor!))) return false;
    }
    if (query.annotation && !node.annotations.some((annotation) => matchesAnnotation(annotation, query.annotation!))) return false;
    if (query.libraryBinding) {
      if (node.type !== 'library-instance'
        || !includes(query.libraryBinding.libraries, node.library)
        || !includes(query.libraryBinding.components, node.component)
        || !includes(query.libraryBinding.variants, node.variant)) return false;
    }
    if (query.componentBinding) {
      if (node.type !== 'component-instance' || !includes(query.componentBinding.mainComponentIds, node.mainComponentId)) return false;
    }
    if (query.variableBinding && !this.matchesVariableBinding(node, query.variableBinding)) return false;
    return true;
  }

  private matchesVariableBinding(node: SceneNode, matcher: SceneVariableBindingMatcher): boolean {
    const bindings = Object.entries(node.variableBindings);
    if (bindings.length === 0) return false;
    const criteria: Array<(propertyPath: string, variableId: string) => boolean> = [];
    if (matcher.variableIds) criteria.push((_propertyPath, variableId) => matcher.variableIds!.includes(variableId));
    if (matcher.collectionIds) criteria.push((_propertyPath, variableId) => {
      const collectionId = this.variableCollectionsByVariableId.get(variableId);
      return collectionId !== undefined && matcher.collectionIds!.includes(collectionId);
    });
    if (matcher.propertyPaths) criteria.push((propertyPath) => matcher.propertyPaths!.includes(propertyPath));
    if (criteria.length === 0) return true;
    if ((matcher.match ?? 'any') === 'all') return criteria.every((criterion) => bindings.some(([path, variableId]) => criterion(path, variableId)));
    return bindings.some(([path, variableId]) => criteria.every((criterion) => criterion(path, variableId)));
  }
}

export function querySceneDocument(document: SceneDocument, query: SceneQuery = {}): SceneQueryResult[] {
  return new SceneQueryIndex(document).query(query);
}
