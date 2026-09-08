import type { DiagramDocument, DiagramEdge, DiagramNode } from './schema.js';

export interface MindMapAnalysis {
  roots: DiagramNode[];
  parentByNode: Map<string, string>;
  childrenByNode: Map<string, DiagramNode[]>;
  multipleParentNodeIds: string[];
  orphanNodeIds: string[];
  cycleNodeIds: string[];
  maxDepth: number;
  maxChildren: number;
  hiddenNodeIds: Set<string>;
}

const horizontalGap = 250;
const verticalGap = 34;

export function isMindMapNode(node: DiagramNode): boolean {
  return node.data.shape === 'mindmap-root' || node.data.shape === 'mindmap-topic';
}

export function mindMapNodeSize(node: DiagramNode): { width: number; height: number } {
  if (node.width && node.height) return { width: node.width, height: node.height };
  const characters = [...node.data.label].length;
  if (node.data.shape === 'mindmap-root') return { width: Math.max(180, Math.min(300, 112 + characters * 10)), height: 64 };
  return { width: Math.max(128, Math.min(240, 78 + characters * 8)), height: 46 };
}

export function analyzeMindMap(document: DiagramDocument): MindMapAnalysis {
  const nodes = document.nodes.filter(isMindMapNode);
  const nodeById = new Map(nodes.map((node) => [node.id, node]));
  const parents = new Map<string, string[]>();
  const childrenByNode = new Map<string, DiagramNode[]>();
  for (const node of nodes) childrenByNode.set(node.id, []);
  for (const edge of document.edges) {
    const source = nodeById.get(edge.source);
    const target = nodeById.get(edge.target);
    if (!source || !target || source.id === target.id) continue;
    parents.set(target.id, [...(parents.get(target.id) ?? []), source.id]);
    childrenByNode.get(source.id)!.push(target);
  }
  for (const children of childrenByNode.values()) {
    children.sort((left, right) => (left.data.mindmapOrder ?? Number.MAX_SAFE_INTEGER) - (right.data.mindmapOrder ?? Number.MAX_SAFE_INTEGER)
      || left.position.y - right.position.y
      || left.id.localeCompare(right.id));
  }
  const parentByNode = new Map<string, string>();
  const multipleParentNodeIds: string[] = [];
  for (const [nodeId, parentIds] of parents) {
    if (parentIds.length > 1) multipleParentNodeIds.push(nodeId);
    if (parentIds[0]) parentByNode.set(nodeId, parentIds[0]);
  }
  const roots = nodes.filter((node) => !parentByNode.has(node.id));
  const orphanNodeIds = document.nodes.filter((node) => !isMindMapNode(node) && node.data.shape !== 'text').map((node) => node.id);
  const cycleNodeIds = new Set<string>();
  const reachedNodeIds = new Set<string>();
  let maxDepth = 0;
  const visit = (nodeId: string, depth: number, path: Set<string>) => {
    reachedNodeIds.add(nodeId);
    maxDepth = Math.max(maxDepth, depth);
    if (path.has(nodeId)) {
      for (const item of path) cycleNodeIds.add(item);
      cycleNodeIds.add(nodeId);
      return;
    }
    const nextPath = new Set(path).add(nodeId);
    for (const child of childrenByNode.get(nodeId) ?? []) visit(child.id, depth + 1, nextPath);
  };
  for (const root of roots) visit(root.id, 0, new Set());
  if (roots.length === 0 && nodes[0]) visit(nodes[0].id, 0, new Set());
  for (const node of nodes) if (!reachedNodeIds.has(node.id)) visit(node.id, 0, new Set());
  const hiddenNodeIds = new Set<string>();
  const hideDescendants = (nodeId: string) => {
    for (const child of childrenByNode.get(nodeId) ?? []) {
      if (hiddenNodeIds.has(child.id)) continue;
      hiddenNodeIds.add(child.id);
      hideDescendants(child.id);
    }
  };
  for (const node of nodes) if (node.data.mindmapCollapsed) hideDescendants(node.id);
  return {
    roots,
    parentByNode,
    childrenByNode,
    multipleParentNodeIds,
    orphanNodeIds,
    cycleNodeIds: [...cycleNodeIds],
    maxDepth,
    maxChildren: Math.max(0, ...[...childrenByNode.values()].map((children) => children.length)),
    hiddenNodeIds
  };
}

export function layoutMindMap(document: DiagramDocument): DiagramDocument {
  const next = structuredClone(document);
  const analysis = analyzeMindMap(next);
  const root = analysis.roots.find((node) => node.data.shape === 'mindmap-root') ?? analysis.roots[0];
  if (!root) return next;
  root.data.shape = 'mindmap-root';
  delete root.data.mindmapSide;
  root.width = mindMapNodeSize(root).width;
  root.height = mindMapNodeSize(root).height;

  const visibleChildren = (nodeId: string) => (analysis.childrenByNode.get(nodeId) ?? []).filter((node) => !analysis.hiddenNodeIds.has(node.id));
  const branchHeight = (node: DiagramNode, path = new Set<string>()): number => {
    if (path.has(node.id)) return mindMapNodeSize(node).height;
    const children = node.data.mindmapCollapsed ? [] : visibleChildren(node.id);
    if (children.length === 0) return mindMapNodeSize(node).height;
    const nextPath = new Set(path).add(node.id);
    return Math.max(mindMapNodeSize(node).height, children.reduce((sum, child) => sum + branchHeight(child, nextPath), 0) + verticalGap * (children.length - 1));
  };

  const rootChildren = visibleChildren(root.id);
  rootChildren.forEach((node, index) => {
    if (!node.data.mindmapSide) node.data.mindmapSide = index % 2 === 0 ? 'right' : 'left';
    node.data.mindmapOrder ??= index;
  });
  const placeSide = (side: 'left' | 'right') => {
    const branches = rootChildren.filter((node) => node.data.mindmapSide === side);
    const totalHeight = branches.reduce((sum, node) => sum + branchHeight(node), 0) + Math.max(0, branches.length - 1) * verticalGap;
    let cursorY = -totalHeight / 2;
    const placeBranch = (node: DiagramNode, depth: number, centerY: number, path: Set<string>) => {
      if (path.has(node.id)) return;
      const size = mindMapNodeSize(node);
      node.width = size.width;
      node.height = size.height;
      node.data.shape = 'mindmap-topic';
      node.data.category = 'mindmap';
      node.data.mindmapSide = side;
      node.position = {
        x: side === 'right' ? depth * horizontalGap : -depth * horizontalGap - size.width,
        y: centerY - size.height / 2
      };
      const children = node.data.mindmapCollapsed ? [] : visibleChildren(node.id);
      if (children.length === 0) return;
      const childTotal = children.reduce((sum, child) => sum + branchHeight(child), 0) + verticalGap * (children.length - 1);
      let childY = centerY - childTotal / 2;
      const nextPath = new Set(path).add(node.id);
      children.forEach((child, index) => {
        child.data.mindmapOrder ??= index;
        const height = branchHeight(child);
        placeBranch(child, depth + 1, childY + height / 2, nextPath);
        childY += height + verticalGap;
      });
    };
    for (const branch of branches) {
      const height = branchHeight(branch);
      placeBranch(branch, 1, cursorY + height / 2, new Set([root.id]));
      cursorY += height + verticalGap;
    }
  };
  placeSide('left');
  placeSide('right');
  root.position = { x: -root.width / 2, y: -root.height / 2 };
  normalizeMindMapOrigin(next);
  next.edges = next.edges.map((edge) => mindMapEdge(edge, next.nodes));
  return next;
}

export function createMindMapEdge(source: string, target: string, nodes: DiagramNode[], id = `mindmap-edge-${crypto.randomUUID().slice(0, 8)}`): DiagramEdge {
  return mindMapEdge({ id, source, target }, nodes);
}

export function mindMapEdge(edge: DiagramEdge, nodes: DiagramNode[]): DiagramEdge {
  const target = nodes.find((node) => node.id === edge.target);
  const side = target?.data.mindmapSide ?? 'right';
  const next: DiagramEdge = {
    ...edge,
    sourceHandle: side === 'right' ? 'right' : 'left',
    targetHandle: side === 'right' ? 'left' : 'right',
    type: 'bezier',
    data: {
      ...edge.data,
      lineStyle: 'solid',
      startMarker: 'none',
      endMarker: 'none',
      strokeWidth: edge.data?.strokeWidth ?? 2.2,
      color: edge.data?.color ?? target?.data.color ?? '#6C7BD9'
    }
  };
  delete next.label;
  if (next.data) delete next.data.relation;
  return next;
}

export function mindMapSubtreeIds(document: DiagramDocument, nodeId: string): Set<string> {
  const analysis = analyzeMindMap(document);
  const ids = new Set<string>();
  const visit = (id: string) => {
    if (ids.has(id)) return;
    ids.add(id);
    for (const child of analysis.childrenByNode.get(id) ?? []) visit(child.id);
  };
  visit(nodeId);
  return ids;
}

function normalizeMindMapOrigin(document: DiagramDocument): void {
  const hiddenNodeIds = analyzeMindMap(document).hiddenNodeIds;
  const visible = document.nodes.filter((node) => !hiddenNodeIds.has(node.id));
  if (visible.length === 0) return;
  const minX = Math.min(...visible.map((node) => node.position.x));
  const minY = Math.min(...visible.map((node) => node.position.y));
  const offsetX = 100 - minX;
  const offsetY = 100 - minY;
  for (const node of document.nodes) node.position = { x: node.position.x + offsetX, y: node.position.y + offsetY };
}
