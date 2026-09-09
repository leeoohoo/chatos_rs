import {
  createSceneNodeBase,
  indexSceneDocument,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneComponentInstanceNode,
  type SceneDocument,
  type SceneGroupNode,
  type SceneNode
} from '../../src/v2/scene-schema';

export interface SceneSnippet {
  id: string;
  name: string;
  nodes: SceneNode[];
  width: number;
  height: number;
  createdAt: string;
  updatedAt: string;
}

function nestedCollections(node: SceneNode): SceneNode[][] {
  const collections: SceneNode[][] = [];
  if (isSceneContainer(node)) collections.push(node.children);
  if (isSceneSlotContainer(node)) collections.push(...Object.values(node.slots));
  return collections;
}

function selectedRoots(document: SceneDocument, selectedIds: readonly string[]): SceneNode[] {
  const index = indexSceneDocument(document);
  const selected = new Set(selectedIds);
  const entries = selectedIds.map((id) => index.get(id)).filter((entry) => Boolean(entry));
  if (entries.length === 0) throw new Error('请先选择要保存的 Scene 图层。');
  const rootEntries = entries.filter((entry) => {
    let parentId = entry!.parentId;
    while (index.has(parentId)) {
      if (selected.has(parentId)) return false;
      parentId = index.get(parentId)!.parentId;
    }
    return true;
  });
  const parentKeys = new Set(rootEntries.map((entry) => entry!.parentId));
  if (parentKeys.size !== 1) throw new Error('保存组合前，请先把所选图层放进同一个 Frame、Group 或组件内容区。');
  return rootEntries.map((entry) => structuredClone(entry!.node));
}

function allNodeIds(nodes: readonly SceneNode[]): Set<string> {
  const ids = new Set<string>();
  const visit = (node: SceneNode) => {
    ids.add(node.id);
    for (const collection of nestedCollections(node)) collection.forEach(visit);
  };
  nodes.forEach(visit);
  return ids;
}

function cloneWithFreshIds(nodes: readonly SceneNode[]): SceneNode[] {
  const sourceIds = allNodeIds(nodes);
  const idMap = new Map([...sourceIds].map((id) => [id, `snippet:${crypto.randomUUID()}`]));
  const now = new Date().toISOString();
  const clone = (source: SceneNode): SceneNode => {
    const node = structuredClone(source);
    node.id = idMap.get(source.id)!;
    node.annotations = [];
    node.variableBindings = {};
    delete node.prototypeLink;
    node.createdBy = 'human';
    node.updatedBy = 'human';
    node.createdAt = now;
    node.updatedAt = now;
    if (isSceneContainer(node)) node.children = node.children.map(clone) as typeof node.children;
    if (isSceneSlotContainer(node)) {
      node.slots = Object.fromEntries(Object.entries(node.slots).map(([slot, children]) => [slot, children.map(clone)]));
    }
    if (node.type === 'component-instance') {
      const replacement = idMap.get(node.mainComponentId);
      if (!replacement) throw new Error('组件实例依赖的主组件没有一起保存，请先选择完整组件定义。');
      (node as SceneComponentInstanceNode).mainComponentId = replacement;
    }
    return node;
  };
  return nodes.map(clone);
}

function normalizedNodes(nodes: SceneNode[]): { nodes: SceneNode[]; width: number; height: number } {
  const minX = Math.min(...nodes.map((node) => node.frame.x));
  const minY = Math.min(...nodes.map((node) => node.frame.y));
  const maxX = Math.max(...nodes.map((node) => node.frame.x + node.frame.width));
  const maxY = Math.max(...nodes.map((node) => node.frame.y + node.frame.height));
  for (const node of nodes) node.frame = { ...node.frame, x: node.frame.x - minX, y: node.frame.y - minY };
  return { nodes, width: Math.max(1, maxX - minX), height: Math.max(1, maxY - minY) };
}

export function createSceneSnippet(document: SceneDocument, selectedIds: readonly string[], name: string): SceneSnippet {
  const trimmedName = name.trim();
  if (!trimmedName) throw new Error('请填写组合名称。');
  const now = new Date().toISOString();
  const normalized = normalizedNodes(selectedRoots(document, selectedIds));
  return {
    id: `scene-snippet:${crypto.randomUUID()}`,
    name: trimmedName.slice(0, 240),
    ...normalized,
    createdAt: now,
    updatedAt: now
  };
}

export function instantiateSceneSnippet(snippet: SceneSnippet, x: number, y: number): SceneNode {
  const nodes = cloneWithFreshIds(snippet.nodes);
  const targetX = Math.round(x);
  const targetY = Math.round(y);
  if (nodes.length === 1) {
    nodes[0].frame = { ...nodes[0].frame, x: targetX, y: targetY };
    return nodes[0];
  }
  const group = createSceneNodeBase('group', snippet.name, {
    x: targetX,
    y: targetY,
    width: snippet.width,
    height: snippet.height
  }) as SceneGroupNode;
  group.layout.position = 'absolute';
  group.children = nodes;
  return group;
}

export function parseSceneSnippets(source: string | null | undefined): SceneSnippet[] {
  if (!source) return [];
  try {
    const parsed = JSON.parse(source) as SceneSnippet[];
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((snippet) => snippet && typeof snippet.id === 'string' && typeof snippet.name === 'string'
      && Array.isArray(snippet.nodes) && Number.isFinite(snippet.width) && Number.isFinite(snippet.height));
  } catch {
    return [];
  }
}
