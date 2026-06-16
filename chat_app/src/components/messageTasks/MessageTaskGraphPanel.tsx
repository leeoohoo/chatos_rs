import { memo, useMemo, useState, type FC, type MouseEvent } from 'react';
import { Activity, FileText } from 'lucide-react';
import type { Edge, Node } from '@xyflow/react';
import type {
  MessageTaskRunnerGraphEdge,
  MessageTaskRunnerGraphNode,
  MessageTaskRunnerGraphResponse,
  MessageTaskRunnerTask,
} from '../../lib/api/client/types';
import { cn } from '../../lib/utils';
import {
  TASK_GRAPH_NODE_HEIGHT,
  TASK_GRAPH_NODE_WIDTH,
  layoutMessageTaskGraph,
  type TaskGraphLayoutPoint,
} from './messageTaskGraphLayout';
import { StatusBadge } from './parts';
import { readString, readStringArray } from './utils';

interface MessageTaskGraphPanelProps {
  graph: MessageTaskRunnerGraphResponse;
  loading: boolean;
  error: string | null;
  loadingRunId: string | null;
  panelWidth: number;
  onOpenDetail: (task: MessageTaskRunnerTask) => void;
  onOpenRun: (task: MessageTaskRunnerTask) => void | Promise<void>;
}

interface TaskGraphNodeData extends Record<string, unknown> {
  graphNode: MessageTaskRunnerGraphNode;
  currentSourceUserMessageId: string | null;
  isActive: boolean;
  isDimmed: boolean;
  loadingRun: boolean;
  onSelectTask: (taskId: string) => void;
  onOpenDetail: (task: MessageTaskRunnerTask) => void;
  onOpenRun: (task: MessageTaskRunnerTask) => void | Promise<void>;
}

type TaskGraphFlowNode = Node<TaskGraphNodeData, 'task'>;
type TaskGraphEdgeData = {
  stroke: string;
  animated: boolean;
  markerId: string;
  layoutPoints?: TaskGraphLayoutPoint[];
};
type TaskGraphFlowEdge = Edge<TaskGraphEdgeData>;

interface PositionedTaskNode {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  data: TaskGraphNodeData;
  zIndex: number;
}

const CANVAS_PADDING = 32;
const VIEW_PADDING = 40;

const descriptionClampStyle = {
  display: 'block',
  overflow: 'hidden',
  maxHeight: '7.5rem',
  whiteSpace: 'pre-wrap' as const,
};

const stopNodeButtonEvent = (event: MouseEvent<HTMLButtonElement>) => {
  event.stopPropagation();
};

const normalizeStatus = (status?: string | null): string | null => (
  readString(status)?.toLowerCase() || null
);

const isRunningTask = (task?: Pick<MessageTaskRunnerTask, 'status'> | null): boolean => (
  normalizeStatus(task?.status) === 'running'
);

const shortId = (value: string): string => (
  value.length > 16 ? `${value.slice(0, 6)}...${value.slice(-4)}` : value
);

const relationshipLabel = (graphNode: MessageTaskRunnerGraphNode): string => {
  if (graphNode.is_current_message) {
    return '当前消息';
  }
  if (graphNode.depth <= 1) {
    return '直接前置';
  }
  return '间接前置';
};

const relationshipTone = (graphNode: MessageTaskRunnerGraphNode): string => {
  if (graphNode.is_current_message) {
    return 'border-primary/25 bg-primary/10 text-primary';
  }
  if (graphNode.depth <= 1) {
    return 'border-amber-300/80 bg-amber-50 text-amber-700 dark:border-amber-400/40 dark:bg-amber-500/10 dark:text-amber-200';
  }
  return 'border-border bg-muted/60 text-muted-foreground';
};

const cardTone = (graphNode: MessageTaskRunnerGraphNode): string => {
  if (graphNode.is_current_message) {
    return 'border-primary/35 bg-[linear-gradient(180deg,rgba(239,246,255,0.98),rgba(255,255,255,0.96))] shadow-[0_14px_38px_-28px_rgba(37,99,235,0.95)] dark:bg-[linear-gradient(180deg,rgba(30,41,59,0.98),rgba(15,23,42,0.94))]';
  }
  if (graphNode.depth <= 1) {
    return 'border-amber-300/80 bg-[linear-gradient(180deg,rgba(255,251,235,0.98),rgba(255,255,255,0.96))] dark:border-amber-400/35 dark:bg-[linear-gradient(180deg,rgba(69,26,3,0.42),rgba(15,23,42,0.9))]';
  }
  return 'border-border/90 bg-card/95';
};

const walkTaskIds = (originId: string, adjacency: Map<string, string[]>): Set<string> => {
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

const getNodeDimensions = (node: TaskGraphFlowNode) => ({
  width: typeof node.style?.width === 'number' ? node.style.width : TASK_GRAPH_NODE_WIDTH,
  height: typeof node.style?.height === 'number' ? node.style.height : TASK_GRAPH_NODE_HEIGHT,
});

const buildFlowNodes = (
  graphNodes: MessageTaskRunnerGraphResponse['nodes'],
  currentSourceUserMessageId: string | null,
  activeTaskId: string | null,
  relatedTaskIds: Set<string> | null,
  loadingRunId: string | null,
  onSelectTask: (taskId: string | null) => void,
  onOpenDetail: (task: MessageTaskRunnerTask) => void,
  onOpenRun: (task: MessageTaskRunnerTask) => void | Promise<void>,
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
      loadingRun: Boolean(graphNode.task.last_run_id && graphNode.task.last_run_id === loadingRunId),
      onSelectTask: (taskId) => onSelectTask(activeTaskId === taskId ? null : taskId),
      onOpenDetail,
      onOpenRun,
    },
    style: {
      width: TASK_GRAPH_NODE_WIDTH,
      height: TASK_GRAPH_NODE_HEIGHT,
    },
    zIndex: activeTaskId === graphNode.task.id ? 30 : graphNode.is_current_message ? 20 : 10,
  }))
);

const buildFlowEdges = (
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

const edgePath = (
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

const MessageTaskCardNode = memo(({ node }: { node: PositionedTaskNode }) => {
  const {
    currentSourceUserMessageId,
    graphNode,
    isActive,
    isDimmed,
    loadingRun,
    onSelectTask,
    onOpenDetail,
    onOpenRun,
  } = node.data;
  const { task } = graphNode;
  const isRunning = isRunningTask(task);
  const description = readString(task.description)
    || readString(task.objective)
    || '暂无描述';
  const prerequisiteCount = Array.isArray(task.prerequisite_task_ids)
    ? task.prerequisite_task_ids.length
    : 0;
  const sourceUserMessageId = readString(task.source_user_message_id);
  const showSourceHint = sourceUserMessageId && sourceUserMessageId !== currentSourceUserMessageId;

  return (
    <article
      className={cn(
        'relative overflow-hidden rounded-xl border p-3 shadow-sm backdrop-blur-sm transition-all duration-150',
        cardTone(graphNode),
        isActive && 'ring-2 ring-primary/35 shadow-[0_22px_45px_-30px_rgba(37,99,235,0.9)]',
        isDimmed && 'opacity-40 saturate-50',
        isRunning && 'message-task-running-card',
      )}
      style={{
        width: node.width,
        height: node.height,
      }}
    >
      {isRunning ? (
        <div className="message-task-running-card-border pointer-events-none absolute inset-0 rounded-[inherit]" />
      ) : null}
      <div className="flex h-full flex-col overflow-hidden">
        <div className="shrink-0 flex flex-wrap items-center gap-2">
          <span
            className={cn(
              'rounded-full border px-2 py-0.5 text-[11px] font-medium',
              relationshipTone(graphNode),
            )}
          >
            {relationshipLabel(graphNode)}
          </span>
          <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[11px] text-muted-foreground">
            深度 {graphNode.depth}
          </span>
          <StatusBadge status={task.status} />
        </div>

        <div className="mt-3 min-h-0 flex-1 overflow-hidden">
          <h3 className="break-words text-sm font-semibold leading-5 text-foreground">
            {task.title || task.id}
          </h3>
          {showSourceHint ? (
            <p className="mt-1 truncate text-[11px] text-muted-foreground">
              源消息 {shortId(sourceUserMessageId)}
            </p>
          ) : null}
          <p
            className="mt-2 break-words text-sm leading-5 text-muted-foreground"
            style={descriptionClampStyle}
          >
            {description}
          </p>
        </div>

        <div className="mt-3 shrink-0 border-t border-border/80 pt-3">
          <div className="flex items-center justify-between gap-3 text-[11px] text-muted-foreground">
            <span className="truncate">前置依赖 {prerequisiteCount}</span>
            <span className="truncate">
              {task.last_run_id ? '有运行记录' : '暂无运行记录'}
            </span>
          </div>

          <div className="mt-3 grid grid-cols-3 gap-2">
            <button
              type="button"
              className={cn(
                'inline-flex items-center justify-center rounded-md border px-2 py-1.5 text-xs transition-colors',
                isActive
                  ? 'border-primary/30 bg-primary/10 text-primary'
                  : 'border-border bg-background text-muted-foreground hover:bg-accent hover:text-foreground',
              )}
              onMouseDown={stopNodeButtonEvent}
              onClick={(event) => {
                stopNodeButtonEvent(event);
                onSelectTask(task.id);
              }}
            >
              {isActive ? '已聚焦' : '聚焦链路'}
            </button>
            <button
              type="button"
              className="inline-flex items-center justify-center gap-1 rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground hover:bg-accent disabled:opacity-60"
              onMouseDown={stopNodeButtonEvent}
              onClick={(event) => {
                stopNodeButtonEvent(event);
                onOpenDetail(task);
              }}
            >
              <FileText className="h-3.5 w-3.5" />
              详情
            </button>
            <button
              type="button"
              className="inline-flex items-center justify-center gap-1 rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60"
              disabled={loadingRun || !task.last_run_id}
              onMouseDown={stopNodeButtonEvent}
              onClick={(event) => {
                stopNodeButtonEvent(event);
                void onOpenRun(task);
              }}
            >
              <Activity className="h-3.5 w-3.5" />
              运行详情
            </button>
          </div>
        </div>
      </div>
    </article>
  );
});

MessageTaskCardNode.displayName = 'MessageTaskCardNode';

export const MessageTaskGraphPanel: FC<MessageTaskGraphPanelProps> = ({
  graph,
  loading,
  error,
  loadingRunId,
  panelWidth,
  onOpenDetail,
  onOpenRun,
}) => {
  const [activeTaskId, setActiveTaskId] = useState<string | null>(null);

  const displayGraph = useMemo(
    () => normalizeMessageTaskGraphForDisplay(graph),
    [graph],
  );

  const taskById = useMemo(
    () => new Map(displayGraph.nodes.map((node) => [node.task.id, node])),
    [displayGraph.nodes],
  );
  const displayEdges = displayGraph.edges;

  const { parentMap, childMap } = useMemo(() => {
    const nextParentMap = new Map<string, string[]>();
    const nextChildMap = new Map<string, string[]>();
    displayEdges.forEach(({ source, target }) => {
      nextParentMap.set(target, [...(nextParentMap.get(target) || []), source]);
      nextChildMap.set(source, [...(nextChildMap.get(source) || []), target]);
    });
    return {
      parentMap: nextParentMap,
      childMap: nextChildMap,
    };
  }, [displayEdges]);

  const activeContext = useMemo(() => {
    if (!activeTaskId) {
      return null;
    }
    const activeNode = taskById.get(activeTaskId);
    if (!activeNode) {
      return null;
    }
    const upstreamIds = walkTaskIds(activeTaskId, parentMap);
    const downstreamIds = walkTaskIds(activeTaskId, childMap);
    return {
      activeNode,
      upstreamIds,
      downstreamIds,
      relatedTaskIds: new Set<string>([activeTaskId, ...upstreamIds, ...downstreamIds]),
    };
  }, [activeTaskId, childMap, parentMap, taskById]);

  const layout = useMemo(() => {
    const flowNodes = buildFlowNodes(
      displayGraph.nodes,
      readString(graph.source_user_message_id),
      activeTaskId,
      activeContext?.relatedTaskIds || null,
      loadingRunId,
      setActiveTaskId,
      onOpenDetail,
      onOpenRun,
    );
    const flowEdges = buildFlowEdges(
      displayEdges,
      taskById,
      activeTaskId,
      activeContext?.relatedTaskIds || null,
    );
    return layoutMessageTaskGraph(flowNodes, flowEdges);
  }, [
    activeContext?.relatedTaskIds,
    activeTaskId,
    displayGraph.nodes,
    displayEdges,
    graph.source_user_message_id,
    loadingRunId,
    onOpenDetail,
    onOpenRun,
    taskById,
  ]);

  const positionedNodes = useMemo(() => (
    layout.nodes.map((node) => {
      const { width, height } = getNodeDimensions(node);
      return {
        id: node.id,
        x: node.position.x,
        y: node.position.y,
        width,
        height,
        data: node.data,
        zIndex: node.zIndex ?? 10,
      };
    })
  ), [layout.nodes]);

  const nodeLookup = useMemo(
    () => new Map(positionedNodes.map((node) => [node.id, node])),
    [positionedNodes],
  );

  const bounds = useMemo(() => {
    if (!positionedNodes.length) {
      return {
        minX: 0,
        minY: 0,
        width: 0,
        height: 0,
      };
    }
    const minX = Math.min(...positionedNodes.map((node) => node.x));
    const minY = Math.min(...positionedNodes.map((node) => node.y));
    const maxX = Math.max(...positionedNodes.map((node) => node.x + node.width));
    const maxY = Math.max(...positionedNodes.map((node) => node.y + node.height));
    return {
      minX,
      minY,
      width: maxX - minX,
      height: maxY - minY,
    };
  }, [positionedNodes]);

  const contentWidth = Math.max(bounds.width + CANVAS_PADDING * 2, panelWidth - VIEW_PADDING);
  const contentHeight = Math.max(bounds.height + CANVAS_PADDING * 2, 420);
  const offsetX = CANVAS_PADDING - bounds.minX;
  const offsetY = CANVAS_PADDING - bounds.minY;

  if (loading) {
    return (
      <div className="flex h-full min-h-[26rem] items-center justify-center rounded-xl border border-border bg-muted/10">
        <div className="space-y-3 text-center">
          <div className="mx-auto h-10 w-10 animate-pulse rounded-full bg-primary/10" />
          <p className="text-sm text-muted-foreground">正在生成任务流程图...</p>
        </div>
      </div>
    );
  }

  if (!graph.nodes.length) {
    return (
      <div className="flex h-full min-h-[26rem] items-center justify-center rounded-xl border border-dashed border-border bg-muted/10 px-6 text-center">
        <div className="space-y-2">
          <p className="text-sm font-medium text-foreground">这条消息暂无关联任务</p>
          <p className="text-sm text-muted-foreground">
            一旦当前消息触发了任务，这里会把它和前置依赖一起展示成流程图。
          </p>
          {error ? (
            <p className="text-xs text-red-600">{error}</p>
          ) : null}
        </div>
      </div>
    );
  }

  return (
    <div className="relative h-full min-h-[26rem] overflow-hidden rounded-xl border border-border bg-card">
      <div className="absolute left-4 top-4 z-20 flex max-w-[calc(100%-8rem)] flex-col gap-2">
        <div className="flex flex-wrap gap-2 rounded-full border border-border/80 bg-background/88 px-3 py-2 text-[11px] text-muted-foreground shadow-sm backdrop-blur-sm">
          <span className="inline-flex items-center gap-1.5">
            <span className="h-2.5 w-2.5 rounded-full bg-primary/80" />
            当前消息
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span className="h-2.5 w-2.5 rounded-full bg-amber-400/90" />
            直接前置
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span className="h-2.5 w-2.5 rounded-full bg-slate-300" />
            间接前置
          </span>
        </div>
        {activeContext ? (
          <div className="rounded-xl border border-primary/15 bg-background/92 px-3 py-2 text-xs shadow-sm backdrop-blur-sm">
            <div className="font-medium text-foreground">
              正在聚焦：{activeContext.activeNode.task.title || activeContext.activeNode.task.id}
            </div>
            <div className="mt-1 text-muted-foreground">
              上游 {activeContext.upstreamIds.size} 个，下游 {activeContext.downstreamIds.size} 个
            </div>
          </div>
        ) : null}
      </div>

      <div className="absolute right-4 top-4 z-20 flex gap-2">
        {activeTaskId ? (
          <button
            type="button"
            className="rounded-md border border-border bg-background/92 px-3 py-1.5 text-xs text-foreground shadow-sm backdrop-blur-sm hover:bg-accent"
            onClick={() => setActiveTaskId(null)}
          >
            清除聚焦
          </button>
        ) : null}
      </div>

      <div className="h-full overflow-auto px-4 pb-4 pt-24">
        <div
          className="relative mx-auto"
          style={{
            width: contentWidth,
            height: contentHeight,
          }}
        >
          <svg
            className="pointer-events-none absolute inset-0"
            width={contentWidth}
            height={contentHeight}
            viewBox={`0 0 ${contentWidth} ${contentHeight}`}
            aria-hidden
          >
            <defs>
              <marker
                id="task-graph-arrow"
                markerWidth="12"
                markerHeight="12"
                refX="10"
                refY="6"
                orient="auto"
                markerUnits="strokeWidth"
              >
                <path d="M 0 0 L 12 6 L 0 12 z" fill="context-stroke" />
              </marker>
              <marker
                id="task-graph-arrow-running"
                markerWidth="12"
                markerHeight="12"
                refX="10"
                refY="6"
                orient="auto"
                markerUnits="strokeWidth"
              >
                <path
                  className="message-task-running-arrow"
                  d="M 0 0 L 12 6 L 0 12 z"
                  fill="context-stroke"
                />
              </marker>
            </defs>
            {layout.edges.map((edge) => {
              const source = nodeLookup.get(edge.source);
              const target = nodeLookup.get(edge.target);
              if (!source || !target) {
                return null;
              }
              const stroke = edge.data?.stroke || 'rgba(100, 116, 139, 0.72)';
              const layoutPoints = edge.data?.layoutPoints?.map((point) => ({
                x: point.x + offsetX,
                y: point.y + offsetY,
              }));
              return (
                <path
                  key={edge.id}
                  className={edge.data?.animated ? 'message-task-running-edge' : undefined}
                  d={edgePath(
                    { ...source, x: source.x + offsetX, y: source.y + offsetY },
                    { ...target, x: target.x + offsetX, y: target.y + offsetY },
                    layoutPoints,
                  )}
                  fill="none"
                  strokeLinecap="round"
                  style={{
                    stroke,
                    strokeWidth: typeof edge.style?.strokeWidth === 'number' ? edge.style.strokeWidth : 1.6,
                    opacity: typeof edge.style?.opacity === 'number' ? edge.style.opacity : 1,
                  }}
                  markerEnd={`url(#${edge.data?.markerId || 'task-graph-arrow'})`}
                />
              );
            })}
          </svg>

          {positionedNodes.map((node) => (
            <div
              key={node.id}
              className="absolute"
              style={{
                left: node.x + offsetX,
                top: node.y + offsetY,
                width: node.width,
                height: node.height,
                zIndex: node.zIndex,
              }}
            >
              <MessageTaskCardNode node={node} />
            </div>
          ))}
        </div>
      </div>

      {displayEdges.length === 0 ? (
        <div className="pointer-events-none absolute bottom-4 left-4 z-20 rounded-xl border border-border/80 bg-background/88 px-3 py-2 text-xs text-muted-foreground shadow-sm backdrop-blur-sm">
          当前图里没有依赖连线，说明这些任务现在是并列根任务，或还没有建立前置关系。
        </div>
      ) : null}
    </div>
  );
};
