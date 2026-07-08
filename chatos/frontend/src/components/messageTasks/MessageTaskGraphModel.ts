// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Edge, Node } from '@xyflow/react';

import type {
  MessageTaskRunnerGraphEdge,
  MessageTaskRunnerGraphNode,
  MessageTaskRunnerGraphResponse,
  MessageTaskRunnerTask,
} from '../../lib/api/client/types';
import {
  TASK_GRAPH_NODE_HEIGHT,
  TASK_GRAPH_NODE_WIDTH,
  type TaskGraphLayoutPoint,
} from './messageTaskGraphLayout';
import { readString, readStringArray } from './utils';

export interface TaskGraphNodeData extends Record<string, unknown> {
  graphNode: MessageTaskRunnerGraphNode;
  currentSourceUserMessageId: string | null;
  isActive: boolean;
  isDimmed: boolean;
  loadingProcessLog: boolean;
  loadingRun: boolean;
  loadingChanges: boolean;
  onSelectTask: (taskId: string) => void;
  onOpenDetail: (task: MessageTaskRunnerTask) => void;
  onOpenProcessLog: (task: MessageTaskRunnerTask) => void | Promise<void>;
  onOpenRun: (task: MessageTaskRunnerTask) => void | Promise<void>;
  onOpenChanges: (task: MessageTaskRunnerTask) => void | Promise<void>;
}

export type TaskGraphFlowNode = Node<TaskGraphNodeData, 'task'>;
export type TaskGraphEdgeData = {
  stroke: string;
  animated: boolean;
  markerId: string;
  layoutPoints?: TaskGraphLayoutPoint[];
};
export type TaskGraphFlowEdge = Edge<TaskGraphEdgeData>;

export interface PositionedTaskNode {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  data: TaskGraphNodeData;
  zIndex: number;
}

export const CANVAS_PADDING = 32;
export const VIEW_PADDING = 40;

const normalizeStatus = (status?: string | null): string | null => (
  readString(status)?.toLowerCase() || null
);

export const isRunningTask = (
  task?: Pick<MessageTaskRunnerTask, 'status'> | null,
): boolean => (
  normalizeStatus(task?.status) === 'running'
);

export const walkTaskIds = (
  originId: string,
  adjacency: Map<string, string[]>,
): Set<string> => {
  const visited = new Set<string>();
  const queue = [...(adjacency.get(originId) || [])];
  while (queue.length > 0) {
    const current = queue.shift();
    if (!current || visited.has(current)) {
      continue;
    }
    visited.add(current);
    queue.push(...(adjacency.get(current) || []));
  }
  return visited;
};

export const normalizeMessageTaskGraphForDisplay = (
  graph: Pick<MessageTaskRunnerGraphResponse, 'nodes' | 'edges'>,
): Pick<MessageTaskRunnerGraphResponse, 'nodes' | 'edges'> => {
  const nodes = graph.nodes
    .filter((node) => readString(node.task?.id))
    .map((node) => ({ ...node }));
  const nodeById = new Map(
    nodes.map((node) => [node.task.id, node]),
  );

  const edgeByKey = new Map<string, MessageTaskRunnerGraphEdge>();
  const addEdge = (
    source: string | null | undefined,
    target: string | null | undefined,
    kind?: string | null,
  ) => {
    const normalizedSource = readString(source);
    const normalizedTarget = readString(target);
    if (!normalizedSource || !normalizedTarget || normalizedSource === normalizedTarget) {
      return;
    }
    if (!nodeById.has(normalizedSource) || !nodeById.has(normalizedTarget)) {
      return;
    }
    const key = `${normalizedSource}->${normalizedTarget}`;
    if (edgeByKey.has(key)) {
      return;
    }
    edgeByKey.set(key, {
      id: key,
      source: normalizedSource,
      target: normalizedTarget,
      kind: kind || 'prerequisite',
    });
  };

  nodes.forEach((node) => {
    readStringArray(node.task?.prerequisite_task_ids)
      .forEach((prerequisiteTaskId) => addEdge(prerequisiteTaskId, node.task.id, 'prerequisite'));
  });
  if (edgeByKey.size === 0) {
    graph.edges.forEach((edge) => {
      addEdge(edge.source, edge.target, edge.kind);
    });
  }

  const displayEdges = Array.from(edgeByKey.values());
  const depthById = new Map(nodes.map((node) => [node.task.id, 0]));
  for (let iteration = 0; iteration < nodes.length; iteration += 1) {
    let changed = false;
    displayEdges.forEach(({ source, target }) => {
      const targetDepth = depthById.get(target) ?? 0;
      const sourceDepth = depthById.get(source) ?? 0;
      const nextSourceDepth = targetDepth + 1;
      if (nextSourceDepth > sourceDepth) {
        depthById.set(source, nextSourceDepth);
        changed = true;
      }
    });
    if (!changed) {
      break;
    }
  }

  return {
    nodes: nodes.map((node) => ({
      ...node,
      depth: depthById.get(node.task.id) ?? node.depth,
    })),
    edges: displayEdges,
  };
};

export const normalizeMessageTaskGraphEdgesForDisplay = (
  graph: Pick<MessageTaskRunnerGraphResponse, 'nodes' | 'edges'>,
): MessageTaskRunnerGraphEdge[] => {
  return normalizeMessageTaskGraphForDisplay(graph).edges;
};

export const getNodeDimensions = (node: TaskGraphFlowNode) => ({
  width: typeof node.style?.width === 'number' ? node.style.width : TASK_GRAPH_NODE_WIDTH,
  height: typeof node.style?.height === 'number' ? node.style.height : TASK_GRAPH_NODE_HEIGHT,
});

export const buildFlowNodes = (
  graphNodes: MessageTaskRunnerGraphResponse['nodes'],
  currentSourceUserMessageId: string | null,
  activeTaskId: string | null,
  relatedTaskIds: Set<string> | null,
  loadingProcessTaskId: string | null,
  loadingRunId: string | null,
  loadingChangesRunId: string | null,
  onSelectTask: (taskId: string | null) => void,
  onOpenDetail: (task: MessageTaskRunnerTask) => void,
  onOpenProcessLog: (task: MessageTaskRunnerTask) => void | Promise<void>,
  onOpenRun: (task: MessageTaskRunnerTask) => void | Promise<void>,
  onOpenChanges: (task: MessageTaskRunnerTask) => void | Promise<void>,
): TaskGraphFlowNode[] => (
  graphNodes.map((graphNode) => ({
    id: graphNode.task.id,
    type: 'task',
    position: { x: 0, y: 0 },
    draggable: false,
    selectable: false,
    data: {
      currentSourceUserMessageId,
      graphNode,
      isActive: activeTaskId === graphNode.task.id,
      isDimmed: Boolean(activeTaskId && relatedTaskIds && !relatedTaskIds.has(graphNode.task.id)),
      loadingProcessLog: graphNode.task.id === loadingProcessTaskId,
      loadingRun: Boolean(graphNode.task.last_run_id && graphNode.task.last_run_id === loadingRunId),
      loadingChanges: Boolean(graphNode.task.last_run_id && graphNode.task.last_run_id === loadingChangesRunId),
      onSelectTask: (taskId) => onSelectTask(activeTaskId === taskId ? null : taskId),
      onOpenDetail,
      onOpenProcessLog,
      onOpenRun,
      onOpenChanges,
    },
    style: {
      width: TASK_GRAPH_NODE_WIDTH,
      height: TASK_GRAPH_NODE_HEIGHT,
    },
    zIndex: activeTaskId === graphNode.task.id ? 30 : graphNode.is_current_message ? 20 : 10,
  }))
);

export const buildFlowEdges = (
  graphEdges: MessageTaskRunnerGraphResponse['edges'],
  nodeById: Map<string, MessageTaskRunnerGraphNode>,
  activeTaskId: string | null,
  relatedTaskIds: Set<string> | null,
): TaskGraphFlowEdge[] => (
  graphEdges.map((edge) => {
    const isActiveLink = Boolean(activeTaskId && (edge.source === activeTaskId || edge.target === activeTaskId));
    const isHighlighted = Boolean(
      activeTaskId
      && relatedTaskIds
      && relatedTaskIds.has(edge.source)
      && relatedTaskIds.has(edge.target),
    );
    const isRunningEdge = isRunningTask(nodeById.get(edge.source)?.task)
      || isRunningTask(nodeById.get(edge.target)?.task);
    const stroke = isRunningEdge
      ? 'rgba(59, 130, 246, 0.95)'
      : isActiveLink
        ? 'rgba(37, 99, 235, 0.78)'
        : isHighlighted
          ? 'rgba(148, 163, 184, 0.86)'
          : 'rgba(100, 116, 139, 0.72)';
    return {
      id: edge.id,
      source: edge.source,
      target: edge.target,
      style: {
        strokeWidth: isRunningEdge ? 2.5 : isActiveLink ? 2.2 : isHighlighted ? 2 : 1.7,
        opacity: activeTaskId ? (isHighlighted ? 1 : 0.22) : 0.98,
      },
      zIndex: isActiveLink ? 20 : isHighlighted ? 16 : 12,
      data: {
        stroke,
        animated: isRunningEdge,
        markerId: isRunningEdge ? 'task-graph-arrow-running' : 'task-graph-arrow',
      },
    };
  })
);

const roundedPolylinePath = (points: TaskGraphLayoutPoint[]): string => {
  if (points.length === 0) {
    return '';
  }
  if (points.length === 1) {
    return `M ${points[0].x} ${points[0].y}`;
  }
  const commands = [`M ${points[0].x} ${points[0].y}`];
  const radius = 18;

  for (let index = 1; index < points.length - 1; index += 1) {
    const previous = points[index - 1];
    const current = points[index];
    const next = points[index + 1];
    const previousVector = {
      x: current.x - previous.x,
      y: current.y - previous.y,
    };
    const nextVector = {
      x: next.x - current.x,
      y: next.y - current.y,
    };
    const previousLength = Math.hypot(previousVector.x, previousVector.y);
    const nextLength = Math.hypot(nextVector.x, nextVector.y);
    if (previousLength < 1 || nextLength < 1) {
      commands.push(`L ${current.x} ${current.y}`);
      continue;
    }
    const cornerRadius = Math.min(radius, previousLength / 2, nextLength / 2);
    const beforeCorner = {
      x: current.x - (previousVector.x / previousLength) * cornerRadius,
      y: current.y - (previousVector.y / previousLength) * cornerRadius,
    };
    const afterCorner = {
      x: current.x + (nextVector.x / nextLength) * cornerRadius,
      y: current.y + (nextVector.y / nextLength) * cornerRadius,
    };
    commands.push(`L ${beforeCorner.x} ${beforeCorner.y}`);
    commands.push(`Q ${current.x} ${current.y} ${afterCorner.x} ${afterCorner.y}`);
  }

  const last = points[points.length - 1];
  commands.push(`L ${last.x} ${last.y}`);
  return commands.join(' ');
};

export const edgePath = (
  source: PositionedTaskNode,
  target: PositionedTaskNode,
  layoutPoints?: TaskGraphLayoutPoint[],
): string => {
  if (layoutPoints && layoutPoints.length >= 2) {
    return roundedPolylinePath(layoutPoints);
  }
  const startX = source.x + source.width / 2;
  const startY = source.y + source.height;
  const endX = target.x + target.width / 2;
  const endY = target.y;
  const controlOffset = Math.max(56, Math.abs(endY - startY) * 0.45);
  return [
    `M ${startX} ${startY}`,
    `C ${startX} ${startY + controlOffset}, ${endX} ${endY - controlOffset}, ${endX} ${endY}`,
  ].join(' ');
};
