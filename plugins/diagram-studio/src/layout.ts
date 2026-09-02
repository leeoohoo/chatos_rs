import ELK from 'elkjs/lib/elk.bundled.js';
import type { DiagramDocument, DiagramNode } from './schema.js';

const elk = new ELK();

function nodeSize(node: DiagramNode): { width: number; height: number } {
  if (node.width && node.height) return { width: node.width, height: node.height };
  if (node.data.shape === 'lifeline') return { width: 160, height: 560 };
  if (node.data.shape === 'activation') return { width: 14, height: 120 };
  if (node.data.shape === 'fragment') return { width: 620, height: 220 };
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
  const lanes = next.nodes.filter((node) => node.type === 'laneNode');
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
