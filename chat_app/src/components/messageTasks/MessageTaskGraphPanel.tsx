// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo, useState, type FC } from 'react';

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
  walkTaskIds,
} from './MessageTaskGraphModel';
import { MessageTaskCardNode } from './MessageTaskGraphNode';

export {
  normalizeMessageTaskGraphEdgesForDisplay,
  normalizeMessageTaskGraphForDisplay,
} from './MessageTaskGraphModel';

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
