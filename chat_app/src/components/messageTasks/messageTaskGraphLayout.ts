import { Position, type Edge, type Node } from '@xyflow/react';
import dagre from '@dagrejs/dagre';

export const TASK_GRAPH_NODE_WIDTH = 320;
export const TASK_GRAPH_NODE_HEIGHT = 272;

export function layoutMessageTaskGraph<NodeType extends Node, EdgeType extends Edge = Edge>(
  nodes: NodeType[],
  edges: EdgeType[],
): { nodes: NodeType[]; edges: EdgeType[] } {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({
    rankdir: 'TB',
    align: 'UL',
    nodesep: 36,
    ranksep: 96,
    marginx: 24,
    marginy: 24,
  });

  nodes.forEach((node) => {
    graph.setNode(node.id, {
      width: node.width ?? TASK_GRAPH_NODE_WIDTH,
      height: node.height ?? TASK_GRAPH_NODE_HEIGHT,
    });
  });

  edges.forEach((edge) => {
    graph.setEdge(edge.source, edge.target);
  });

  dagre.layout(graph);

  return {
    nodes: nodes.map((node) => {
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
    }),
    edges,
  };
}
