// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  useCallback,
  useMemo,
  useRef,
  useState,
  type FC,
  type WheelEvent,
} from 'react';
import { Maximize2, Minus, Plus } from 'lucide-react';

import type {
  MessageTaskRunnerGraphResponse,
  MessageTaskRunnerTask,
} from '../../lib/api/client/types';
import { layoutMessageTaskGraph } from './messageTaskGraphLayout';
import { readString } from './utils';
import {
  CANVAS_PADDING,
  VIEW_PADDING,
  buildFlowEdges,
  buildFlowNodes,
  edgePath,
  getNodeDimensions,
  normalizeMessageTaskGraphForDisplay,
  type TaskGraphDisplayMode,
  walkTaskIds,
} from './MessageTaskGraphModel';
import { MessageTaskCardNode } from './MessageTaskGraphNode';

export {
  normalizeMessageTaskGraphEdgesForDisplay,
  normalizeMessageTaskGraphForDisplay,
} from './MessageTaskGraphModel';

export const TASK_GRAPH_MIN_ZOOM = 0.5;
export const TASK_GRAPH_MAX_ZOOM = 1.6;
export const TASK_GRAPH_ZOOM_STEP = 0.1;

export const clampTaskGraphZoom = (zoom: number): number => (
  Math.min(TASK_GRAPH_MAX_ZOOM, Math.max(TASK_GRAPH_MIN_ZOOM, Math.round(zoom * 10) / 10))
);

export const calculateTaskGraphFitZoom = (
  availableWidth: number,
  availableHeight: number,
  contentWidth: number,
  contentHeight: number,
): number => {
  if (availableWidth <= 0 || availableHeight <= 0 || contentWidth <= 0 || contentHeight <= 0) {
    return 1;
  }
  const rawZoom = Math.min(
    1,
    availableWidth / contentWidth,
    availableHeight / contentHeight,
  );
  return Math.max(TASK_GRAPH_MIN_ZOOM, Math.floor(rawZoom * 10) / 10);
};

interface MessageTaskGraphPanelProps {
  graph: MessageTaskRunnerGraphResponse;
  loading: boolean;
  error: string | null;
  loadingRunId: string | null;
  panelWidth: number;
  loadingProcessTaskId: string | null;
  onOpenDetail: (task: MessageTaskRunnerTask) => void;
  onOpenProcessLog: (task: MessageTaskRunnerTask) => void | Promise<void>;
  onOpenRun: (task: MessageTaskRunnerTask) => void | Promise<void>;
}

export const MessageTaskGraphPanel: FC<MessageTaskGraphPanelProps> = ({
  graph,
  loading,
  error,
  loadingRunId,
  loadingProcessTaskId,
  panelWidth,
  onOpenDetail,
  onOpenProcessLog,
  onOpenRun,
}) => {
  const [activeTaskId, setActiveTaskId] = useState<string | null>(null);
  const [displayMode, setDisplayMode] = useState<TaskGraphDisplayMode>('reduced');
  const [zoom, setZoom] = useState(1);
  const scrollContainerRef = useRef<HTMLDivElement>(null);

  const displayGraph = useMemo(
    () => normalizeMessageTaskGraphForDisplay(graph, displayMode),
    [displayMode, graph],
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
    const focusTaskIds = new Set<string>([
      activeTaskId,
      ...(parentMap.get(activeTaskId) || []),
      ...(childMap.get(activeTaskId) || []),
    ]);
    return {
      activeNode,
      upstreamIds,
      downstreamIds,
      focusTaskIds,
      relatedTaskIds: new Set<string>([activeTaskId, ...upstreamIds, ...downstreamIds]),
    };
  }, [activeTaskId, childMap, parentMap, taskById]);

  const layout = useMemo(() => {
    const flowNodes = buildFlowNodes(
      displayGraph.nodes,
      readString(graph.source_user_message_id),
      activeTaskId,
      activeContext?.relatedTaskIds || null,
      activeContext?.focusTaskIds || null,
      loadingProcessTaskId,
      loadingRunId,
      setActiveTaskId,
      onOpenDetail,
      onOpenProcessLog,
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
    activeContext?.focusTaskIds,
    activeTaskId,
    displayGraph.nodes,
    displayEdges,
    graph.source_user_message_id,
    loadingProcessTaskId,
    loadingRunId,
    onOpenDetail,
    onOpenProcessLog,
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
    const routedEdgePoints = layout.edges.flatMap((edge) => edge.data?.layoutPoints || []);
    const minX = Math.min(
      ...positionedNodes.map((node) => node.x),
      ...routedEdgePoints.map((point) => point.x),
    );
    const minY = Math.min(
      ...positionedNodes.map((node) => node.y),
      ...routedEdgePoints.map((point) => point.y),
    );
    const maxX = Math.max(
      ...positionedNodes.map((node) => node.x + node.width),
      ...routedEdgePoints.map((point) => point.x),
    );
    const maxY = Math.max(
      ...positionedNodes.map((node) => node.y + node.height),
      ...routedEdgePoints.map((point) => point.y),
    );
    return {
      minX,
      minY,
      width: maxX - minX,
      height: maxY - minY,
    };
  }, [layout.edges, positionedNodes]);

  const contentWidth = Math.max(bounds.width + CANVAS_PADDING * 2, panelWidth - VIEW_PADDING);
  const contentHeight = Math.max(bounds.height + CANVAS_PADDING * 2, 420);
  const offsetX = CANVAS_PADDING - bounds.minX;
  const offsetY = CANVAS_PADDING - bounds.minY;

  const updateZoom = useCallback((nextZoom: number) => {
    setZoom(clampTaskGraphZoom(nextZoom));
  }, []);

  const fitGraphToView = useCallback(() => {
    const container = scrollContainerRef.current;
    if (!container) return;
    const nextZoom = calculateTaskGraphFitZoom(
      Math.max(0, container.clientWidth - 32),
      Math.max(0, container.clientHeight - 112),
      contentWidth,
      contentHeight,
    );
    setZoom(nextZoom);
    window.requestAnimationFrame(() => {
      if (typeof container.scrollTo === 'function') {
        container.scrollTo({ left: 0, top: 0, behavior: 'smooth' });
      }
    });
  }, [contentHeight, contentWidth]);

  const handleGraphWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    if (!event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
    setZoom((currentZoom) => clampTaskGraphZoom(
      currentZoom + (event.deltaY > 0 ? -TASK_GRAPH_ZOOM_STEP : TASK_GRAPH_ZOOM_STEP),
    ));
  }, []);

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
    <div className="relative h-full min-h-[26rem] overflow-hidden rounded-xl border border-border bg-card bg-[radial-gradient(circle_at_center,rgba(148,163,184,0.14)_1px,transparent_1px)] bg-[length:20px_20px]">
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
          {displayMode === 'full' ? (
            <span className="inline-flex items-center gap-1.5">
              <span className="w-4 border-t border-dashed border-slate-400" />
              上下文关联（不阻塞）
            </span>
          ) : null}
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
        <div className="inline-flex rounded-md border border-border bg-background/92 p-0.5 text-xs shadow-sm backdrop-blur-sm">
          {(['reduced', 'full'] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              className={`rounded px-2.5 py-1 ${displayMode === mode ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:bg-accent hover:text-foreground'}`}
              onClick={() => {
                setActiveTaskId(null);
                setDisplayMode(mode);
              }}
            >
              {mode === 'reduced' ? '精简图' : '完整图'}
            </button>
          ))}
        </div>
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

      <div
        ref={scrollContainerRef}
        className="h-full overflow-auto overscroll-contain px-4 pb-4 pt-24"
        onWheel={handleGraphWheel}
      >
        <div
          className="relative mx-auto"
          style={{
            width: contentWidth * zoom,
            height: contentHeight * zoom,
          }}
        >
          <div
            data-testid="message-task-graph-canvas"
            className="relative origin-top-left will-change-transform transition-transform duration-150 ease-out motion-reduce:transition-none"
            style={{
              width: contentWidth,
              height: contentHeight,
              transform: `scale(${zoom})`,
              transformOrigin: 'top left',
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
                  markerWidth="9"
                  markerHeight="9"
                  refX="8"
                  refY="4.5"
                  orient="auto"
                  markerUnits="strokeWidth"
                >
                  <path d="M 0 0 L 9 4.5 L 0 9 z" fill="context-stroke" />
                </marker>
                <marker
                  id="task-graph-arrow-running"
                  markerWidth="9"
                  markerHeight="9"
                  refX="8"
                  refY="4.5"
                  orient="auto"
                  markerUnits="strokeWidth"
                >
                  <path
                    className="message-task-running-arrow"
                    d="M 0 0 L 9 4.5 L 0 9 z"
                    fill="context-stroke"
                  />
                </marker>
                <marker
                  id="task-graph-arrow-focus"
                  markerWidth="9"
                  markerHeight="9"
                  refX="8"
                  refY="4.5"
                  orient="auto"
                  markerUnits="strokeWidth"
                >
                  <path
                    className="message-task-focus-arrow"
                    d="M 0 0 L 9 4.5 L 0 9 z"
                    fill="context-stroke"
                  />
                </marker>
                <marker
                  id="task-graph-arrow-context"
                  markerWidth="8"
                  markerHeight="8"
                  refX="7"
                  refY="4"
                  orient="auto"
                  markerUnits="strokeWidth"
                >
                  <path d="M 0 0 L 8 4 L 0 8 z" fill="context-stroke" />
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
                const path = edgePath(
                  { ...source, x: source.x + offsetX, y: source.y + offsetY },
                  { ...target, x: target.x + offsetX, y: target.y + offsetY },
                  layoutPoints,
                );
                const strokeWidth = typeof edge.style?.strokeWidth === 'number'
                  ? edge.style.strokeWidth
                  : 1.6;
                const opacity = typeof edge.style?.opacity === 'number' ? edge.style.opacity : 1;
                const edgeClassName = [
                  edge.data?.animated ? 'message-task-running-edge' : null,
                  edge.data?.focusAnimated ? 'message-task-focus-edge' : null,
                ].filter(Boolean).join(' ') || undefined;
                return (
                  <g key={edge.id}>
                    <path
                      d={path}
                      fill="none"
                      stroke="hsl(var(--card))"
                      strokeWidth={strokeWidth + 5}
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      opacity={Math.min(0.92, opacity + 0.18)}
                    />
                    <path
                      data-testid={`message-task-edge-${edge.id}`}
                      className={edgeClassName}
                      d={path}
                      fill="none"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      style={{
                        stroke,
                        strokeWidth,
                        opacity,
                        strokeDasharray: edge.data?.dashArray,
                      }}
                      markerEnd={`url(#${edge.data?.markerId || 'task-graph-arrow'})`}
                    />
                  </g>
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
      </div>

      <div className="absolute bottom-4 right-4 z-30 inline-flex items-center rounded-lg border border-border bg-background/94 p-1 text-xs text-foreground shadow-md backdrop-blur-sm">
        <button
          type="button"
          className="inline-flex h-8 w-8 items-center justify-center rounded-md hover:bg-accent disabled:cursor-not-allowed disabled:opacity-40"
          aria-label="缩小流程图"
          title="缩小（Ctrl/⌘ + 滚轮）"
          disabled={zoom <= TASK_GRAPH_MIN_ZOOM}
          onClick={() => updateZoom(zoom - TASK_GRAPH_ZOOM_STEP)}
        >
          <Minus className="h-4 w-4" />
        </button>
        <button
          type="button"
          className="min-w-[3.75rem] rounded-md px-2 py-1.5 tabular-nums hover:bg-accent"
          aria-label="重置流程图缩放"
          title="恢复到 100%"
          onClick={() => updateZoom(1)}
        >
          {Math.round(zoom * 100)}%
        </button>
        <button
          type="button"
          className="inline-flex h-8 w-8 items-center justify-center rounded-md hover:bg-accent disabled:cursor-not-allowed disabled:opacity-40"
          aria-label="放大流程图"
          title="放大（Ctrl/⌘ + 滚轮）"
          disabled={zoom >= TASK_GRAPH_MAX_ZOOM}
          onClick={() => updateZoom(zoom + TASK_GRAPH_ZOOM_STEP)}
        >
          <Plus className="h-4 w-4" />
        </button>
        <span className="mx-1 h-5 w-px bg-border" />
        <button
          type="button"
          className="inline-flex h-8 items-center justify-center gap-1.5 rounded-md px-2 hover:bg-accent"
          aria-label="适应流程图视图"
          title="把整个流程图缩放到当前窗口"
          onClick={fitGraphToView}
        >
          <Maximize2 className="h-3.5 w-3.5" />
          适应
        </button>
      </div>

      {displayEdges.length === 0 ? (
        <div className="pointer-events-none absolute bottom-4 left-4 z-20 rounded-xl border border-border/80 bg-background/88 px-3 py-2 text-xs text-muted-foreground shadow-sm backdrop-blur-sm">
          当前图里没有依赖连线，说明这些任务现在是并列根任务，或还没有建立前置关系。
        </div>
      ) : null}
    </div>
  );
};
