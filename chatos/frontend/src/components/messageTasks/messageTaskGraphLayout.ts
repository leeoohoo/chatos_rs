// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Position, type Edge, type Node } from '@xyflow/react';
import dagre from '@dagrejs/dagre';

export const TASK_GRAPH_NODE_WIDTH = 320;
export const TASK_GRAPH_NODE_HEIGHT = 300;

export interface TaskGraphLayoutPoint {
  x: number;
  y: number;
}

interface TaskGraphLayoutBox {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

const edgeKind = (edge: Edge): string => {
  const data = edge.data as { kind?: unknown } | undefined;
  return typeof data?.kind === 'string' ? data.kind : 'prerequisite';
};

const compareByOppositeNode = (
  nodeById: Map<string, TaskGraphLayoutBox>,
  side: 'source' | 'target',
) => (left: Edge, right: Edge): number => {
  const leftId = side === 'source' ? left.target : left.source;
  const rightId = side === 'source' ? right.target : right.source;
  const leftNode = nodeById.get(leftId);
  const rightNode = nodeById.get(rightId);
  const leftCenter = leftNode ? leftNode.x + leftNode.width / 2 : 0;
  const rightCenter = rightNode ? rightNode.x + rightNode.width / 2 : 0;
  return leftCenter - rightCenter || left.id.localeCompare(right.id);
};

const portOffset = (index: number, count: number, width: number): number => {
  if (count <= 1) {
    return width / 2;
  }
  const usableWidth = Math.max(48, width - 88);
  const spacing = Math.min(42, usableWidth / (count - 1));
  return width / 2 + (index - (count - 1) / 2) * spacing;
};

const contextRoute = (
  source: TaskGraphLayoutBox,
  target: TaskGraphLayoutBox,
  laneIndex: number,
): TaskGraphLayoutPoint[] => {
  const targetIsRight = target.x + target.width / 2 >= source.x + source.width / 2;
  const direction = targetIsRight ? 1 : -1;
  const start = {
    x: targetIsRight ? source.x + source.width : source.x,
    y: source.y + source.height / 2,
  };
  const end = {
    x: targetIsRight ? target.x : target.x + target.width,
    y: target.y + target.height / 2,
  };
  const sourceOuterX = start.x + direction * (28 + (laneIndex % 3) * 8);
  const targetOuterX = end.x - direction * (28 + (laneIndex % 3) * 8);
  const laneY = Math.min(source.y, target.y) - 32 - (laneIndex % 4) * 12;
  return [
    start,
    { x: sourceOuterX, y: start.y },
    { x: sourceOuterX, y: laneY },
    { x: targetOuterX, y: laneY },
    { x: targetOuterX, y: end.y },
    end,
  ];
};

const routeTaskGraphEdges = <EdgeType extends Edge>(
  boxes: TaskGraphLayoutBox[],
  edges: EdgeType[],
): Map<string, TaskGraphLayoutPoint[]> => {
  const nodeById = new Map(boxes.map((node) => [node.id, node]));
  const outgoing = new Map<string, EdgeType[]>();
  const incoming = new Map<string, EdgeType[]>();
  edges.forEach((edge) => {
    if (edgeKind(edge) === 'context') {
      return;
    }
    outgoing.set(edge.source, [...(outgoing.get(edge.source) || []), edge]);
    incoming.set(edge.target, [...(incoming.get(edge.target) || []), edge]);
  });
  outgoing.forEach((nodeEdges) => nodeEdges.sort(compareByOppositeNode(nodeById, 'source')));
  incoming.forEach((nodeEdges) => nodeEdges.sort(compareByOppositeNode(nodeById, 'target')));

  const sourcePortByEdge = new Map<string, number>();
  const targetPortByEdge = new Map<string, number>();
  outgoing.forEach((nodeEdges, nodeId) => {
    const node = nodeById.get(nodeId);
    if (!node) return;
    nodeEdges.forEach((edge, index) => {
      sourcePortByEdge.set(edge.id, node.x + portOffset(index, nodeEdges.length, node.width));
    });
  });
  incoming.forEach((nodeEdges, nodeId) => {
    const node = nodeById.get(nodeId);
    if (!node) return;
    nodeEdges.forEach((edge, index) => {
      targetPortByEdge.set(edge.id, node.x + portOffset(index, nodeEdges.length, node.width));
    });
  });

  const hardEdges = edges.filter((edge) => edgeKind(edge) !== 'context');
  const bandEdges = new Map<string, EdgeType[]>();
  hardEdges.forEach((edge) => {
    const source = nodeById.get(edge.source);
    const target = nodeById.get(edge.target);
    if (!source || !target) return;
    const key = `${Math.round(source.y + source.height)}:${Math.round(target.y)}`;
    bandEdges.set(key, [...(bandEdges.get(key) || []), edge]);
  });
  bandEdges.forEach((nodeEdges) => nodeEdges.sort((left, right) => {
    const leftMid = ((sourcePortByEdge.get(left.id) || 0) + (targetPortByEdge.get(left.id) || 0)) / 2;
    const rightMid = ((sourcePortByEdge.get(right.id) || 0) + (targetPortByEdge.get(right.id) || 0)) / 2;
    return leftMid - rightMid || left.id.localeCompare(right.id);
  }));

  const routes = new Map<string, TaskGraphLayoutPoint[]>();
  let contextIndex = 0;
  edges.forEach((edge) => {
    const source = nodeById.get(edge.source);
    const target = nodeById.get(edge.target);
    if (!source || !target) return;
    if (edgeKind(edge) === 'context') {
      routes.set(edge.id, contextRoute(source, target, contextIndex));
      contextIndex += 1;
      return;
    }

    const start = {
      x: sourcePortByEdge.get(edge.id) ?? source.x + source.width / 2,
      y: source.y + source.height,
    };
    const end = {
      x: targetPortByEdge.get(edge.id) ?? target.x + target.width / 2,
      y: target.y,
    };
    const gap = end.y - start.y;
    if (gap <= 36) {
      const direction = end.x >= start.x ? 1 : -1;
      const sideX = direction > 0
        ? Math.max(source.x + source.width, target.x + target.width) + 36
        : Math.min(source.x, target.x) - 36;
      routes.set(edge.id, [
        start,
        { x: start.x, y: start.y + 24 },
        { x: sideX, y: start.y + 24 },
        { x: sideX, y: end.y - 24 },
        { x: end.x, y: end.y - 24 },
        end,
      ]);
      return;
    }
    const key = `${Math.round(start.y)}:${Math.round(end.y)}`;
    const siblings = bandEdges.get(key) || [edge];
    const laneIndex = Math.max(0, siblings.findIndex((candidate) => candidate.id === edge.id));
    const maxLaneOffset = Math.max(0, gap / 2 - 28);
    const laneOffset = Math.max(
      -maxLaneOffset,
      Math.min(maxLaneOffset, (laneIndex - (siblings.length - 1) / 2) * 10),
    );
    const laneY = start.y + gap / 2 + laneOffset;
    routes.set(edge.id, [
      start,
      { x: start.x, y: laneY },
      { x: end.x, y: laneY },
      end,
    ]);
  });
  return routes;
};

export function layoutMessageTaskGraph<NodeType extends Node, EdgeType extends Edge = Edge>(
  nodes: NodeType[],
  edges: EdgeType[],
): { nodes: NodeType[]; edges: EdgeType[] } {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({
    rankdir: 'TB',
    align: 'UL',
    acyclicer: 'greedy',
    ranker: 'network-simplex',
    nodesep: 64,
    edgesep: 28,
    ranksep: 132,
    marginx: 32,
    marginy: 32,
  });

  nodes.forEach((node) => {
    graph.setNode(node.id, {
      width: node.width ?? TASK_GRAPH_NODE_WIDTH,
      height: node.height ?? TASK_GRAPH_NODE_HEIGHT,
    });
  });

  edges.forEach((edge) => {
    if (edgeKind(edge) === 'context') {
      return;
    }
    graph.setEdge(edge.source, edge.target, {
      edgeId: edge.id,
      weight: 8,
      minlen: 1,
    });
  });

  dagre.layout(graph);

  const layoutNodes = nodes.map((node) => {
      const layoutNode = graph.node(node.id);
      const width = node.width ?? TASK_GRAPH_NODE_WIDTH;
      const height = node.height ?? TASK_GRAPH_NODE_HEIGHT;
      return {
        ...node,
        sourcePosition: Position.Bottom,
        targetPosition: Position.Top,
        position: {
          x: layoutNode.x - width / 2,
          y: layoutNode.y - height / 2,
        },
      };
    });
  const routes = routeTaskGraphEdges(
    layoutNodes.map((node) => ({
      id: node.id,
      x: node.position.x,
      y: node.position.y,
      width: node.width ?? TASK_GRAPH_NODE_WIDTH,
      height: node.height ?? TASK_GRAPH_NODE_HEIGHT,
    })),
    edges,
  );

  return {
    nodes: layoutNodes,
    edges: edges.map((edge) => {
      return {
        ...edge,
        data: {
          ...(edge.data || {}),
          layoutPoints: routes.get(edge.id),
        },
      };
    }) as EdgeType[],
  };
}
