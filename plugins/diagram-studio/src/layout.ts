import ELK from 'elkjs/lib/elk.bundled.js';
import type { ElkNode } from 'elkjs/lib/elk-api.js';
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
    try {
      await layoutCompoundDiagram(next, direction ?? 'RIGHT');
    } catch (error) {
      if (typeof process !== 'undefined' && process.env.DIAGRAM_STUDIO_LAYOUT_DEBUG === '1') {
        console.error('Diagram Studio compound layout failed; using fallback.', error);
      }
      // Keep existing documents usable if ELK rejects malformed legacy nesting.
      layoutContainerDiagramFallback(next, direction ?? 'RIGHT');
    }
    refreshGenericEdgeHandles(next);
    return next;
  }

  const resolvedDirection = direction ?? (document.kind === 'flowchart' ? 'DOWN' : 'RIGHT');
  const shouldWrapArchitecture = (document.kind === 'architecture' || document.kind === 'topology')
    && next.nodes.length >= 6;
  const graph = await elk.layout({
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': resolvedDirection,
      'elk.spacing.nodeNode': '72',
      'elk.layered.spacing.nodeNodeBetweenLayers': '96',
      'elk.padding': '[top=50,left=50,bottom=50,right=50]',
      'elk.layered.nodePlacement.strategy': 'NETWORK_SIMPLEX',
      ...(shouldWrapArchitecture
        ? {
            'elk.layered.wrapping.strategy': 'MULTI_EDGE',
            'elk.aspectRatio': '1.6'
          }
        : {})
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
  if (document.kind === 'architecture' || document.kind === 'topology') refreshGenericEdgeHandles(next);
  return next;
}

async function layoutCompoundDiagram(document: DiagramDocument, direction: 'RIGHT' | 'DOWN'): Promise<void> {
  const shouldWrap = document.nodes.length >= 8;
  const nodeById = new Map(document.nodes.map((node) => [node.id, node]));
  const childrenByParent = new Map<string | undefined, DiagramNode[]>();
  for (const node of document.nodes) {
    const children = childrenByParent.get(node.parentId) ?? [];
    children.push(node);
    childrenByParent.set(node.parentId, children);
  }
  const toElkNode = (node: DiagramNode): ElkNode => {
    const size = nodeSize(node);
    const children = childrenByParent.get(node.id) ?? [];
    return {
      id: node.id,
      width: node.width ?? size.width,
      height: node.height ?? size.height,
      ...(children.length > 0
        ? {
            layoutOptions: {
              'elk.algorithm': 'layered',
              'elk.direction': direction === 'RIGHT' ? 'DOWN' : 'RIGHT',
              'elk.edgeRouting': 'ORTHOGONAL',
              'elk.padding': '[top=62,left=30,bottom=30,right=30]',
              'elk.spacing.nodeNode': '56',
              'elk.layered.spacing.nodeNodeBetweenLayers': '72',
              'elk.layered.nodePlacement.strategy': 'NETWORK_SIMPLEX'
            },
            children: children.map(toElkNode)
          }
        : {})
    };
  };
  const graph: ElkNode = {
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': direction,
      'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
      'elk.edgeRouting': 'ORTHOGONAL',
      'elk.padding': '[top=56,left=56,bottom=56,right=56]',
      'elk.spacing.nodeNode': '84',
      'elk.spacing.edgeNode': '34',
      'elk.layered.spacing.nodeNodeBetweenLayers': '128',
      'elk.layered.spacing.edgeNodeBetweenLayers': '34',
      'elk.layered.nodePlacement.strategy': 'NETWORK_SIMPLEX',
      'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
      'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
      'elk.layered.cycleBreaking.strategy': 'GREEDY',
      ...(shouldWrap
        ? {
            'elk.layered.wrapping.strategy': 'MULTI_EDGE',
            'elk.aspectRatio': '1.8'
          }
        : {})
    },
    children: (childrenByParent.get(undefined) ?? []).map(toElkNode),
    edges: document.edges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target]
    }))
  };
  const laidOut = await elk.layout(graph);
  const applyNodeLayout = (nodes: ElkNode[] | undefined) => {
    for (const elkNode of nodes ?? []) {
      const node = nodeById.get(elkNode.id);
      if (node) {
        node.position = { x: elkNode.x ?? node.position.x, y: elkNode.y ?? node.position.y };
        if (Number.isFinite(elkNode.width)) node.width = elkNode.width;
        if (Number.isFinite(elkNode.height)) node.height = elkNode.height;
      }
      applyNodeLayout(elkNode.children);
    }
  };
  applyNodeLayout(laidOut.children);
}

function layoutContainerDiagramFallback(document: DiagramDocument, direction: 'RIGHT' | 'DOWN'): void {
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
    const directChild = (nodeId: string): DiagramNode | undefined => {
      let current = nodeById.get(nodeId);
      const seen = new Set<string>();
      while (current?.parentId && !seen.has(current.id)) {
        seen.add(current.id);
        if (current.parentId === container.id) return current;
        current = nodeById.get(current.parentId);
      }
      return undefined;
    };
    const seenChildEdges = new Set<string>();
    const childEdges = document.edges.flatMap((edge) => {
      const source = directChild(edge.source);
      const target = directChild(edge.target);
      if (!source || !target || source.id === target.id) return [];
      const key = `${source.id}\u0000${target.id}`;
      if (seenChildEdges.has(key)) return [];
      seenChildEdges.add(key);
      return [{ ...edge, source: source.id, target: target.id }];
    });
    const ranks = graphRanks(children, childEdges);
    const rankValues = [...new Set(children.map((child) => ranks.get(child.id) ?? 0))].sort((left, right) => left - right);
    let x = 34;
    let y = 70;
    let rowHeight = 0;
    let contentRight = 34;
    let contentBottom = 70;
    const maximumRowWidth = 1540;
    for (const rank of rankValues) {
      const column = children.filter((child) => (ranks.get(child.id) ?? 0) === rank);
      const columnWidth = Math.max(...column.map((child) => child.width ?? nodeSize(child).width));
      const columnHeight = column.reduce((height, child, index) => height + (child.height ?? nodeSize(child).height) + (index > 0 ? 58 : 0), 0);
      if (x > 34 && x + columnWidth > maximumRowWidth) {
        x = 34;
        y += rowHeight + 90;
        rowHeight = 0;
      }
      let childY = y;
      for (const child of column) {
        child.position = { x, y: childY };
        childY += (child.height ?? nodeSize(child).height) + 58;
      }
      rowHeight = Math.max(rowHeight, columnHeight);
      contentRight = Math.max(contentRight, x + columnWidth);
      contentBottom = Math.max(contentBottom, y + columnHeight);
      x += columnWidth + 76;
    }
    container.width = Math.max(300, contentRight + 34);
    container.height = Math.max(170, contentBottom + 34);
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
      secondaryOffset += (direction === 'RIGHT' ? height : width) + 110;
      maximumPrimarySize = Math.max(maximumPrimarySize, direction === 'RIGHT' ? width : height);
    }
    primaryOffset += maximumPrimarySize + 160;
  }
}

function refreshGenericEdgeHandles(document: DiagramDocument): void {
  document.edges = document.edges.map((edge) => {
    const source = document.nodes.find((node) => node.id === edge.source);
    const target = document.nodes.find((node) => node.id === edge.target);
    if (!source || !target) return edge;
    const sourcePosition = absoluteNodePosition(document.nodes, source);
    const targetPosition = absoluteNodePosition(document.nodes, target);
    const sourceSize = nodeSize(source);
    const targetSize = nodeSize(target);
    const sourceCenter = { x: sourcePosition.x + sourceSize.width / 2, y: sourcePosition.y + sourceSize.height / 2 };
    const targetCenter = { x: targetPosition.x + targetSize.width / 2, y: targetPosition.y + targetSize.height / 2 };
    const vertical = Math.abs(targetCenter.y - sourceCenter.y) >= Math.abs(targetCenter.x - sourceCenter.x);
    return {
      ...edge,
      sourceHandle: vertical ? (targetCenter.y >= sourceCenter.y ? 'bottom' : 'top') : (targetCenter.x >= sourceCenter.x ? 'right' : 'left'),
      targetHandle: vertical ? (targetCenter.y >= sourceCenter.y ? 'top' : 'bottom') : (targetCenter.x >= sourceCenter.x ? 'left' : 'right')
    };
  });
  distributeEdgeHandles(document);
}

function distributeEdgeHandles(document: DiagramDocument): void {
  type Endpoint = {
    edgeIndex: number;
    endpoint: 'source' | 'target';
    nodeId: string;
    otherNodeId: string;
    side: 'left' | 'right' | 'top' | 'bottom';
  };
  const nodeById = new Map(document.nodes.map((node) => [node.id, node]));
  const groups = new Map<string, Endpoint[]>();
  const addEndpoint = (endpoint: Endpoint) => {
    const key = `${endpoint.nodeId}\u0000${endpoint.side}`;
    const entries = groups.get(key) ?? [];
    entries.push(endpoint);
    groups.set(key, entries);
  };
  document.edges.forEach((edge, edgeIndex) => {
    addEndpoint({
      edgeIndex,
      endpoint: 'source',
      nodeId: edge.source,
      otherNodeId: edge.target,
      side: baseHandleSide(edge.sourceHandle)
    });
    addEndpoint({
      edgeIndex,
      endpoint: 'target',
      nodeId: edge.target,
      otherNodeId: edge.source,
      side: baseHandleSide(edge.targetHandle)
    });
  });
  const center = (nodeId: string) => {
    const node = nodeById.get(nodeId);
    if (!node) return { x: 0, y: 0 };
    const position = absoluteNodePosition(document.nodes, node);
    const size = nodeSize(node);
    return { x: position.x + size.width / 2, y: position.y + size.height / 2 };
  };
  for (const endpoints of groups.values()) {
    if (endpoints.length < 2) continue;
    endpoints.sort((left, right) => {
      const leftCenter = center(left.otherNodeId);
      const rightCenter = center(right.otherNodeId);
      return left.side === 'left' || left.side === 'right'
        ? leftCenter.y - rightCenter.y
        : leftCenter.x - rightCenter.x;
    });
    endpoints.forEach((endpoint, index) => {
      const slot = Math.round(index * 6 / Math.max(1, endpoints.length - 1));
      const handle = `${endpoint.side}-${slot}`;
      const edge = document.edges[endpoint.edgeIndex];
      if (endpoint.endpoint === 'source') edge.sourceHandle = handle;
      else edge.targetHandle = handle;
    });
  }
}

function baseHandleSide(handle: string | undefined): 'left' | 'right' | 'top' | 'bottom' {
  const side = handle?.split('-', 1)[0];
  return side === 'left' || side === 'top' || side === 'bottom' ? side : 'right';
}

function absoluteNodePosition(nodes: DiagramNode[], node: DiagramNode): { x: number; y: number } {
  if (!node.parentId) return node.position;
  const parent = nodes.find((candidate) => candidate.id === node.parentId);
  if (!parent) return node.position;
  const parentPosition = absoluteNodePosition(nodes, parent);
  return { x: parentPosition.x + node.position.x, y: parentPosition.y + node.position.y };
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
  for (const node of nodes) {
    if (visited.has(node.id)) continue;
    const cycleQueue = [node.id];
    visited.add(node.id);
    while (cycleQueue.length) {
      const current = cycleQueue.shift()!;
      for (const target of outgoing.get(current) ?? []) {
        if (visited.has(target)) continue;
        ranks.set(target, Math.max(ranks.get(target) ?? 0, (ranks.get(current) ?? 0) + 1));
        visited.add(target);
        cycleQueue.push(target);
      }
    }
  }
  return ranks;
}
