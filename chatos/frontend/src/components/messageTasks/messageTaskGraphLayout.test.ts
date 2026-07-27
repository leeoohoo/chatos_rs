// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Edge, Node } from '@xyflow/react';
import { describe, expect, it } from 'vitest';

import {
  TASK_GRAPH_NODE_HEIGHT,
  TASK_GRAPH_NODE_WIDTH,
  layoutMessageTaskGraph,
  type TaskGraphLayoutPoint,
} from './messageTaskGraphLayout';

type LayoutEdgeData = {
  kind: string;
  layoutPoints?: TaskGraphLayoutPoint[];
};

const node = (id: string): Node => ({
  id,
  position: { x: 0, y: 0 },
  data: {},
  width: TASK_GRAPH_NODE_WIDTH,
  height: TASK_GRAPH_NODE_HEIGHT,
});

const edge = (
  id: string,
  source: string,
  target: string,
  kind = 'prerequisite',
): Edge<LayoutEdgeData> => ({
  id,
  source,
  target,
  data: { kind },
});

describe('layoutMessageTaskGraph', () => {
  it('fans multiple connections across separate node ports and routing lanes', () => {
    const layout = layoutMessageTaskGraph(
      [node('source-a'), node('source-b'), node('target-a'), node('target-b')],
      [
        edge('a-to-a', 'source-a', 'target-a'),
        edge('a-to-b', 'source-a', 'target-b'),
        edge('b-to-b', 'source-b', 'target-b'),
      ],
    );
    const routes = new Map(layout.edges.map((item) => [item.id, item.data?.layoutPoints || []]));
    const firstOutgoing = routes.get('a-to-a') || [];
    const secondOutgoing = routes.get('a-to-b') || [];
    const secondIncoming = routes.get('b-to-b') || [];

    expect(firstOutgoing).toHaveLength(4);
    expect(secondOutgoing).toHaveLength(4);
    expect(firstOutgoing[0].x).not.toBe(secondOutgoing[0].x);
    expect(secondOutgoing[secondOutgoing.length - 1]?.x).not.toBe(
      secondIncoming[secondIncoming.length - 1]?.x,
    );
    expect(firstOutgoing[1].y).not.toBe(secondOutgoing[1].y);
  });

  it('routes context-only links around node sides instead of changing hard-dependency ranks', () => {
    const layout = layoutMessageTaskGraph(
      [node('left'), node('right')],
      [edge('context-link', 'left', 'right', 'context')],
    );
    const source = layout.nodes.find((item) => item.id === 'left');
    const route = layout.edges[0].data?.layoutPoints || [];

    expect(route).toHaveLength(6);
    expect(route[0].y).toBe((source?.position.y || 0) + TASK_GRAPH_NODE_HEIGHT / 2);
    expect(route[0].x).toBe((source?.position.x || 0) + TASK_GRAPH_NODE_WIDTH);
  });
});
