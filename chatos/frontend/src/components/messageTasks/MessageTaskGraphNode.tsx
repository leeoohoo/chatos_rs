// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  memo,
  useEffect,
  useRef,
  useState,
  type MouseEvent,
} from 'react';
import { Activity, CircleAlert, FileText, ScrollText } from 'lucide-react';

import { cn } from '../../lib/utils';
import { StatusBadge } from './parts';
import { readString } from './utils';
import {
  isRunningTask,
  type PositionedTaskNode,
} from './MessageTaskGraphModel';

const descriptionClampStyle = {
  display: 'block',
  overflow: 'hidden',
  maxHeight: '7.5rem',
  whiteSpace: 'pre-wrap' as const,
};

const stopNodeControlEvent = (event: MouseEvent<HTMLElement>) => {
  event.stopPropagation();
};

const shortId = (value: string): string => (
  value.length > 16 ? `${value.slice(0, 6)}...${value.slice(-4)}` : value
);

const relationshipLabel = (graphNode: PositionedTaskNode['data']['graphNode']): string => {
  if (graphNode.is_current_message) {
    return '当前消息';
  }
  if (graphNode.depth <= 1) {
    return '直接前置';
  }
  return '间接前置';
};

const relationshipTone = (graphNode: PositionedTaskNode['data']['graphNode']): string => {
  if (graphNode.is_current_message) {
    return 'border-primary/25 bg-primary/10 text-primary';
  }
  if (graphNode.depth <= 1) {
    return 'border-amber-300/80 bg-amber-50 text-amber-700 dark:border-amber-400/40 dark:bg-amber-500/10 dark:text-amber-200';
  }
  return 'border-border bg-muted/60 text-muted-foreground';
};

const displayStatusForTask = (
  task: PositionedTaskNode['data']['graphNode']['task'],
  prerequisiteCount: number,
): string | null | undefined => {
  const status = readString(task.status)?.toLowerCase();
  const hasIncompletePrerequisite = prerequisiteCount > 0
    && (task.prerequisite_tasks || []).some(
      (prerequisite) => readString(prerequisite.status)?.toLowerCase() !== 'succeeded',
    );
  if (status === 'ready' && hasIncompletePrerequisite) {
    return 'waiting_prerequisite';
  }
  return task.status;
};

const cardTone = (graphNode: PositionedTaskNode['data']['graphNode']): string => {
  if (graphNode.is_current_message) {
    return 'border-primary/35 bg-[linear-gradient(180deg,rgba(239,246,255,0.98),rgba(255,255,255,0.96))] shadow-[0_14px_38px_-28px_rgba(37,99,235,0.95)] dark:bg-[linear-gradient(180deg,rgba(30,41,59,0.98),rgba(15,23,42,0.94))]';
  }
  if (graphNode.depth <= 1) {
    return 'border-amber-300/80 bg-[linear-gradient(180deg,rgba(255,251,235,0.98),rgba(255,255,255,0.96))] dark:border-amber-400/35 dark:bg-[linear-gradient(180deg,rgba(69,26,3,0.42),rgba(15,23,42,0.9))]';
  }
  return 'border-border/90 bg-card/95';
};

export const MessageTaskCardNode = memo(({ node }: { node: PositionedTaskNode }) => {
  const {
    currentSourceUserMessageId,
    graphNode,
    isActive,
    isFocusEmphasized,
    isDimmed,
    loadingProcessLog,
    loadingRun,
    onOpenDetail,
    onOpenProcessLog,
    onOpenRun,
    onSelectTask,
  } = node.data;
  const { task } = graphNode;
  const groupedTasks = graphNode.groupedTasks?.length ? graphNode.groupedTasks : [task];
  const [selectedStageTaskId, setSelectedStageTaskId] = useState(task.id);
  const attentionTaskId = groupedTasks.find(
    (stageTask) => readString(stageTask.status)?.toLowerCase() === 'failed',
  )?.id || groupedTasks.find(
    (stageTask) => readString(stageTask.status)?.toLowerCase() === 'blocked',
  )?.id || null;
  const previousAttentionTaskIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (!groupedTasks.some((stageTask) => stageTask.id === selectedStageTaskId)) {
      setSelectedStageTaskId(task.id);
    }
  }, [groupedTasks, selectedStageTaskId, task.id]);
  useEffect(() => {
    if (attentionTaskId && attentionTaskId !== previousAttentionTaskIdRef.current) {
      setSelectedStageTaskId(attentionTaskId);
    }
    previousAttentionTaskIdRef.current = attentionTaskId;
  }, [attentionTaskId]);
  const actionTask = groupedTasks.find((stageTask) => stageTask.id === selectedStageTaskId) || task;
  const actionTaskBlocked = readString(actionTask.status)?.toLowerCase() === 'blocked';
  const isRunning = isRunningTask(task);
  const description = readString(task.description)
    || readString(task.objective)
    || '暂无描述';
  const prerequisiteCount = Array.isArray(task.prerequisite_task_ids)
    ? task.prerequisite_task_ids.length
    : 0;
  const displayStatus = displayStatusForTask(task, prerequisiteCount);
  const sourceUserMessageId = readString(task.source_user_message_id);
  const showSourceHint = sourceUserMessageId && sourceUserMessageId !== currentSourceUserMessageId;

  return (
    <article
      data-testid={`message-task-node-${task.id}`}
      className={cn(
        'relative cursor-pointer overflow-hidden rounded-xl border p-3 shadow-sm backdrop-blur-sm transition-all duration-150',
        cardTone(graphNode),
        isFocusEmphasized && 'message-task-focus-card',
        isActive && 'message-task-focus-card-active',
        isActive && 'ring-2 ring-primary/35 shadow-[0_22px_45px_-30px_rgba(37,99,235,0.9)]',
        isDimmed && 'opacity-40 saturate-50',
        isRunning && 'message-task-running-card',
      )}
      onClick={() => onSelectTask(task.id)}
      style={{
        width: node.width,
        height: node.height,
      }}
    >
      {isRunning && !isFocusEmphasized ? (
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
          <StatusBadge status={displayStatus} />
          {groupedTasks.length > 1 ? (
            <span className="rounded-full border border-violet-200 bg-violet-50 px-2 py-0.5 text-[11px] text-violet-700 dark:border-violet-400/30 dark:bg-violet-500/10 dark:text-violet-200">
              {groupedTasks.length} 个阶段
            </span>
          ) : null}
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
          {groupedTasks.length > 1 ? (
            <label
              className="mt-2 flex items-center gap-2 text-[11px] text-muted-foreground"
              onMouseDown={stopNodeControlEvent}
              onClick={stopNodeControlEvent}
            >
              <span className="shrink-0">查看阶段</span>
              <select
                className="min-w-0 flex-1 rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground"
                value={actionTask.id}
                onChange={(event) => setSelectedStageTaskId(event.target.value)}
              >
                {groupedTasks.map((stageTask, index) => (
                  <option key={stageTask.id} value={stageTask.id}>
                    {index + 1}. {stageTask.title || stageTask.id}
                  </option>
                ))}
              </select>
            </label>
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
              {actionTask.last_run_id ? '有运行记录' : '暂无运行记录'}
            </span>
          </div>

          <div className="mt-3 grid grid-cols-2 gap-2">
            <button
              type="button"
              className="inline-flex items-center justify-center gap-1 rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground hover:bg-accent disabled:cursor-wait disabled:opacity-60"
              disabled={loadingProcessLog}
              onMouseDown={stopNodeControlEvent}
              onClick={(event) => {
                stopNodeControlEvent(event);
                void onOpenProcessLog(actionTask);
              }}
            >
              <ScrollText className="h-3.5 w-3.5" />
              {loadingProcessLog ? '加载中' : '执行过程'}
            </button>
            <button
              type="button"
              className="inline-flex items-center justify-center gap-1 rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground hover:bg-accent disabled:opacity-60"
              onMouseDown={stopNodeControlEvent}
              onClick={(event) => {
                stopNodeControlEvent(event);
                onOpenDetail(actionTask);
              }}
            >
              {actionTaskBlocked
                ? <CircleAlert className="h-3.5 w-3.5 text-orange-600" />
                : <FileText className="h-3.5 w-3.5" />}
              {actionTaskBlocked ? '处理阻塞' : '详情'}
            </button>
            <button
              type="button"
              className="inline-flex items-center justify-center gap-1 rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60"
              disabled={loadingRun || !actionTask.last_run_id}
              onMouseDown={stopNodeControlEvent}
              onClick={(event) => {
                stopNodeControlEvent(event);
                void onOpenRun(actionTask);
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
