import ELK from 'elkjs/lib/elk.bundled.js';
import type { DiagramDocument, DiagramNode } from './schema.js';

const elk = new ELK();

function nodeSize(node: DiagramNode): { width: number; height: number } {
  if (node.width && node.height) return { width: node.width, height: node.height };
  if (node.data.shape === 'lifeline') return { width: 160, height: 560 };
  if (node.data.shape === 'activation') return { width: 14, height: 120 };
  if (node.data.shape === 'fragment') return { width: 620, height: 220 };
  if (node.data.shape === 'container') return { width: 300, height: 180 };
  if (node.data.icon && node.data.showLabel === false) return { width: 72, height: 72 };
  if (node.data.shape === 'diamond') return { width: 150, height: 110 };
  if (node.data.shape === 'circle') return { width: 116, height: 116 };
  if (node.data.shape === 'cylinder') return { width: 180, height: 94 };
  if (node.data.shape === 'text') return { width: 160, height: 44 };
  return { width: 190, height: 82 };
}

export async function layoutDiagram(
  document: DiagramDocument,
  direction?: 'RIGHT' | 'DOWN'
): Promise<DiagramDocument> {
  const next = structuredClone(document);
  if (document.kind === 'sequence') {
    const lifelines = next.nodes
      .filter((node) => node.data.shape === 'lifeline')
      .sort((left, right) => left.position.x - right.position.x);
    lifelines.forEach((lifeline, index) => {
      lifeline.position = { x: 40 + index * 240, y: 30 };
      lifeline.width = lifeline.width ?? 160;
      lifeline.height = Math.max(lifeline.height ?? 560, 420);
    });
    return next;
  }
  const lanes = next.nodes.filter((node) => node.data.shape === 'lane');
  if (lanes.length > 0) {
    lanes.forEach((lane, laneIndex) => {
      lane.position = { x: 30, y: 30 + laneIndex * 200 };
      lane.width = Math.max(lane.width ?? 1120, 900);
      lane.height = Math.max(lane.height ?? 180, 160);
      const children = next.nodes.filter((node) => node.parentId === lane.id);
      children.forEach((child, index) => {
        child.position = { x: 150 + index * 260, y: 48 };
      });
    });
    return next;
  }
  if (next.nodes.some((node) => node.data.shape === 'container')) {
    layoutContainerDiagram(next, direction ?? 'RIGHT');
    return next;
  }

  const resolvedDirection = direction ?? (document.kind === 'flowchart' ? 'DOWN' : 'RIGHT');
  const graph = await elk.layout({
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': resolvedDirection,
      'elk.spacing.nodeNode': '72',
      'elk.layered.spacing.nodeNodeBetweenLayers': '96',
      'elk.padding': '[top=50,left=50,bottom=50,right=50]',
      'elk.layered.nodePlacement.strategy': 'NETWORK_SIMPLEX'
    },
    children: next.nodes.map((node) => ({
      id: node.id,
      ...nodeSize(node)
    })),
    edges: next.edges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target]
    }))
  });
  const positions = new Map(graph.children?.map((node) => [node.id, { x: node.x ?? 0, y: node.y ?? 0 }]) ?? []);
  next.nodes = next.nodes.map((node) => ({
    ...node,
    position: positions.get(node.id) ?? node.position
  }));
  return next;
}

function layoutContainerDiagram(document: DiagramDocument, direction: 'RIGHT' | 'DOWN'): void {
  const nodeById = new Map(document.nodes.map((node) => [node.id, node]));
  const childrenByParent = new Map<string, DiagramNode[]>();
  for (const node of document.nodes) {
    if (!node.parentId) continue;
    const children = childrenByParent.get(node.parentId) ?? [];
    children.push(node);
    childrenByParent.set(node.parentId, children);
  }
  const depth = (node: DiagramNode) => {
    let result = 0;
    let current = node;
    const seen = new Set<string>();
    while (current.parentId && !seen.has(current.parentId)) {
      seen.add(current.parentId);
      result += 1;
      const parent = nodeById.get(current.parentId);
      if (!parent) break;
      current = parent;
    }
    return result;
  };
  const containers = document.nodes
    .filter((node) => node.data.shape === 'container')
    .sort((left, right) => depth(right) - depth(left));
  for (const container of containers) {
    const children = childrenByParent.get(container.id) ?? [];
    if (children.length === 0) {
      container.width = Math.max(container.width ?? 0, 280);
      container.height = Math.max(container.height ?? 0, 150);
      continue;
    }
    const childIds = new Set(children.map((child) => child.id));
    const ranks = graphRanks(children, document.edges.filter((edge) => childIds.has(edge.source) && childIds.has(edge.target)));
    const rankValues = [...new Set(children.map((child) => ranks.get(child.id) ?? 0))].sort((left, right) => left - right);
    let x = 28;
    let contentBottom = 58;
    for (const rank of rankValues) {
      const column = children.filter((child) => (ranks.get(child.id) ?? 0) === rank);
      const columnWidth = Math.max(...column.map((child) => child.width ?? nodeSize(child).width));
      let y = 58;
      for (const child of column) {
        child.position = { x, y };
        y += (child.height ?? nodeSize(child).height) + 34;
        contentBottom = Math.max(contentBottom, y);
      }
      x += columnWidth + 42;
    }
    container.width = Math.max(280, x - 12);
    container.height = Math.max(150, contentBottom + 2);
  }

  const topNode = (nodeId: string): DiagramNode | undefined => {
    let current = nodeById.get(nodeId);
    const seen = new Set<string>();
    while (current?.parentId && !seen.has(current.parentId)) {
      seen.add(current.parentId);
      const parent = nodeById.get(current.parentId);
      if (!parent) break;
      current = parent;
    }
    return current;
  };
  const topNodes = document.nodes.filter((node) => !node.parentId);
  const collapsedEdges = document.edges.flatMap((edge) => {
    const source = topNode(edge.source);
    const target = topNode(edge.target);
    if (!source || !target || source.id === target.id) return [];
    return [{ ...edge, source: source.id, target: target.id }];
  });
  const ranks = graphRanks(topNodes, collapsedEdges);
  const rankValues = [...new Set(topNodes.map((node) => ranks.get(node.id) ?? 0))].sort((left, right) => left - right);
  let primaryOffset = 60;
  for (const rank of rankValues) {
    const group = topNodes.filter((node) => (ranks.get(node.id) ?? 0) === rank);
    let secondaryOffset = 60;
    let maximumPrimarySize = 0;
    for (const node of group) {
      const width = node.width ?? nodeSize(node).width;
      const height = node.height ?? nodeSize(node).height;
      node.position = direction === 'RIGHT'
        ? { x: primaryOffset, y: secondaryOffset }
        : { x: secondaryOffset, y: primaryOffset };
      secondaryOffset += (direction === 'RIGHT' ? height : width) + 68;
      maximumPrimarySize = Math.max(maximumPrimarySize, direction === 'RIGHT' ? width : height);
    }
    primaryOffset += maximumPrimarySize + 110;
  }
}

function graphRanks(nodes: DiagramNode[], edges: DiagramDocument['edges']): Map<string, number> {
  const ranks = new Map(nodes.map((node) => [node.id, 0]));
  const incoming = new Map(nodes.map((node) => [node.id, 0]));
  const outgoing = new Map(nodes.map((node) => [node.id, [] as string[]]));
  const seenEdges = new Set<string>();
  for (const edge of edges) {
    if (!incoming.has(edge.target) || !outgoing.has(edge.source)) continue;
    const key = `${edge.source}\u0000${edge.target}`;
    if (seenEdges.has(key)) continue;
    seenEdges.add(key);
    incoming.set(edge.target, (incoming.get(edge.target) ?? 0) + 1);
    outgoing.get(edge.source)!.push(edge.target);
  }
  const queue = nodes.filter((node) => (incoming.get(node.id) ?? 0) === 0).map((node) => node.id);
  const visited = new Set<string>();
  while (queue.length) {
    const current = queue.shift()!;
    if (visited.has(current)) continue;
    visited.add(current);
    for (const target of outgoing.get(current) ?? []) {
      ranks.set(target, Math.max(ranks.get(target) ?? 0, (ranks.get(current) ?? 0) + 1));
      incoming.set(target, Math.max(0, (incoming.get(target) ?? 0) - 1));
      if ((incoming.get(target) ?? 0) === 0) queue.push(target);
    }
  }
  return ranks;
}
