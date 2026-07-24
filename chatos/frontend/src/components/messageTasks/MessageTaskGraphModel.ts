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
import { isRecord, readString, readStringArray } from './utils';

export type TaskGraphDisplayMode = 'reduced' | 'full';

export interface MessageTaskGraphDisplayNode extends MessageTaskRunnerGraphNode {
  groupedTasks?: MessageTaskRunnerTask[];
  projectTaskId?: string | null;
}

export interface MessageTaskGraphDisplay {
  nodes: MessageTaskGraphDisplayNode[];
  edges: MessageTaskRunnerGraphEdge[];
}

export interface TaskGraphNodeData extends Record<string, unknown> {
  graphNode: MessageTaskGraphDisplayNode;
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
  dashArray?: string;
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

const projectTaskIdForTask = (task: MessageTaskRunnerTask): string | null => {
  if (isRecord(task.input_payload)) {
    const projectTaskId = readString(task.input_payload.project_task_id);
    if (projectTaskId) {
      return projectTaskId;
    }
  }
  return readString(task.project_task_id);
};

const executionClientRefForTask = (task: MessageTaskRunnerTask): string | null => {
  if (isRecord(task.input_payload)) {
    const clientRef = readString(task.input_payload.execution_client_ref);
    if (clientRef) {
      return clientRef;
    }
  }
  return readString(task.execution_client_ref);
};

const dependencyContextRefsForTask = (task: MessageTaskRunnerTask): string[] => {
  if (isRecord(task.input_payload)) {
    const refs = readStringArray(task.input_payload.dependency_context_refs);
    if (refs.length > 0) {
      return refs;
    }
  }
  return readStringArray(task.dependency_context_refs);
};

const edgePathExistsWithout = (
  origin: string,
  target: string,
  adjacency: Map<string, string[]>,
  excludedKey: string,
): boolean => {
  const pending = [origin];
  const visited = new Set<string>();
  while (pending.length > 0) {
    const current = pending.pop();
    if (!current || visited.has(current)) {
      continue;
    }
    visited.add(current);
    for (const next of adjacency.get(current) || []) {
      if (`${current}->${next}` === excludedKey) {
        continue;
      }
      if (next === target) {
        return true;
      }
      pending.push(next);
    }
  }
  return false;
};

const transitiveReduceDisplayEdges = (
  edges: MessageTaskRunnerGraphEdge[],
): MessageTaskRunnerGraphEdge[] => {
  const nodeIds = new Set<string>();
  const indegree = new Map<string, number>();
  const adjacency = new Map<string, string[]>();
  edges.forEach(({ source, target }) => {
    nodeIds.add(source);
    nodeIds.add(target);
    adjacency.set(source, [...(adjacency.get(source) || []), target]);
    indegree.set(source, indegree.get(source) || 0);
    indegree.set(target, (indegree.get(target) || 0) + 1);
  });
  const ready = Array.from(nodeIds).filter((nodeId) => (indegree.get(nodeId) || 0) === 0);
  let visitedCount = 0;
  while (ready.length > 0) {
    const current = ready.pop();
    if (!current) continue;
    visitedCount += 1;
    (adjacency.get(current) || []).forEach((next) => {
      const nextIndegree = (indegree.get(next) || 0) - 1;
      indegree.set(next, nextIndegree);
      if (nextIndegree === 0) ready.push(next);
    });
  }
  if (visitedCount !== nodeIds.size) {
    return edges;
  }
  return edges.filter((edge) => !edgePathExistsWithout(
    edge.source,
    edge.target,
    adjacency,
    `${edge.source}->${edge.target}`,
  ));
};

const aggregateGroupedStatus = (tasks: MessageTaskRunnerTask[]): string | null => {
  const statuses = tasks
    .map((task) => normalizeStatus(task.status))
    .filter((status): status is string => Boolean(status));
  const has = (...values: string[]) => statuses.some((status) => values.includes(status));
  if (has('failed', 'error')) return 'failed';
  if (has('running', 'processing', 'in_progress', 'doing')) return 'running';
  if (has('blocked')) return 'blocked';
  if (has('queued', 'ready', 'todo', 'pending')) return 'ready';
  if (has('cancelled', 'canceled')) return 'cancelled';
  if (statuses.length > 0 && statuses.every((status) => ['succeeded', 'success', 'completed', 'done'].includes(status))) {
    return 'completed';
  }
  return statuses[0] || null;
};

const isReviewTitle = (title?: string | null): boolean => (
  /(^|\b)review(\b|$)|复核|审查|验收/i.test(readString(title) || '')
);

const withRecomputedDepth = (graph: MessageTaskGraphDisplay): MessageTaskGraphDisplay => {
  const depthById = new Map(graph.nodes.map((node) => [node.task.id, 0]));
  for (let iteration = 0; iteration < graph.nodes.length; iteration += 1) {
    let changed = false;
    graph.edges.forEach(({ source, target }) => {
      const targetDepth = depthById.get(target) ?? 0;
      const sourceDepth = depthById.get(source) ?? 0;
      if (targetDepth + 1 > sourceDepth) {
        depthById.set(source, targetDepth + 1);
        changed = true;
      }
    });
    if (!changed) break;
  }
  return {
    nodes: graph.nodes.map((node) => ({
      ...node,
      depth: depthById.get(node.task.id) ?? node.depth,
    })),
    edges: graph.edges,
  };
};

const collapseProjectTaskStages = (graph: MessageTaskGraphDisplay): MessageTaskGraphDisplay => {
  const groups = new Map<string, MessageTaskGraphDisplayNode[]>();
  graph.nodes.forEach((node) => {
    const projectTaskId = projectTaskIdForTask(node.task);
    const key = projectTaskId ? `project:${projectTaskId}` : `task:${node.task.id}`;
    groups.set(key, [...(groups.get(key) || []), node]);
  });

  const representativeByTaskId = new Map<string, string>();
  const nodes = Array.from(groups.values()).map((group) => {
    const representative = group.find((node) => !isReviewTitle(node.task.title)) || group[0];
    const groupedTasks = group.map((node) => node.task);
    group.forEach((node) => representativeByTaskId.set(node.task.id, representative.task.id));
    const projectTaskId = projectTaskIdForTask(representative.task);
    return {
      ...representative,
      is_current_message: group.some((node) => node.is_current_message),
      is_root: group.some((node) => node.is_root),
      projectTaskId,
      groupedTasks,
      task: {
        ...representative.task,
        status: aggregateGroupedStatus(groupedTasks),
      },
    };
  });

  const edgeByKey = new Map<string, MessageTaskRunnerGraphEdge>();
  graph.edges.forEach((edge) => {
    const source = representativeByTaskId.get(edge.source) || edge.source;
    const target = representativeByTaskId.get(edge.target) || edge.target;
    if (source === target) return;
    const key = `${source}->${target}`;
    if (!edgeByKey.has(key)) {
      edgeByKey.set(key, { ...edge, id: key, source, target });
    }
  });
  const edges = transitiveReduceDisplayEdges(Array.from(edgeByKey.values()));
  const prerequisitesByTaskId = new Map<string, string[]>();
  edges.forEach(({ source, target }) => {
    prerequisitesByTaskId.set(target, [...(prerequisitesByTaskId.get(target) || []), source]);
  });
  return withRecomputedDepth({
    nodes: nodes.map((node) => ({
      ...node,
      task: {
        ...node.task,
        prerequisite_task_ids: prerequisitesByTaskId.get(node.task.id) || [],
      },
    })),
    edges,
  });
};

export const normalizeMessageTaskGraphForDisplay = (
  graph: Pick<MessageTaskRunnerGraphResponse, 'nodes' | 'edges'>,
  mode: TaskGraphDisplayMode = 'reduced',
): MessageTaskGraphDisplay => {
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

  if (mode === 'full') {
    const taskIdByClientRef = new Map<string, string>();
    nodes.forEach((node) => {
      const clientRef = executionClientRefForTask(node.task);
      if (clientRef) taskIdByClientRef.set(clientRef, node.task.id);
    });
    nodes.forEach((node) => {
      dependencyContextRefsForTask(node.task).forEach((contextRef) => {
        addEdge(taskIdByClientRef.get(contextRef), node.task.id, 'context');
      });
    });
  }

  const displayEdges = Array.from(edgeByKey.values());
  const normalized = withRecomputedDepth({
    nodes,
    edges: mode === 'reduced' ? transitiveReduceDisplayEdges(displayEdges) : displayEdges,
  });
  return mode === 'reduced' ? collapseProjectTaskStages(normalized) : normalized;
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
  graphNodes: MessageTaskGraphDisplayNode[],
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
  graphNodes.map((graphNode) => {
    const groupedTasks = graphNode.groupedTasks?.length ? graphNode.groupedTasks : [graphNode.task];
    return {
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
        loadingProcessLog: groupedTasks.some((task) => task.id === loadingProcessTaskId),
        loadingRun: groupedTasks.some((task) => Boolean(task.last_run_id && task.last_run_id === loadingRunId)),
        loadingChanges: groupedTasks.some((task) => Boolean(task.last_run_id && task.last_run_id === loadingChangesRunId)),
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
    };
  })
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
    const isContextEdge = edge.kind === 'context';
    const stroke = isContextEdge
      ? 'rgba(148, 163, 184, 0.62)'
      : isRunningEdge
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
        animated: !isContextEdge && isRunningEdge,
        markerId: isRunningEdge ? 'task-graph-arrow-running' : 'task-graph-arrow',
        dashArray: isContextEdge ? '7 7' : undefined,
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
