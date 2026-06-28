import { memo, type MouseEvent } from 'react';
import { Activity, FileText, ScrollText } from 'lucide-react';

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

const stopNodeButtonEvent = (event: MouseEvent<HTMLButtonElement>) => {
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
    isDimmed,
    loadingProcessLog,
    loadingRun,
    onOpenDetail,
    onOpenProcessLog,
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
              className="inline-flex items-center justify-center gap-1 rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground hover:bg-accent disabled:cursor-wait disabled:opacity-60"
              disabled={loadingProcessLog}
              onMouseDown={stopNodeButtonEvent}
              onClick={(event) => {
                stopNodeButtonEvent(event);
                void onOpenProcessLog(task);
              }}
            >
              <ScrollText className="h-3.5 w-3.5" />
              {loadingProcessLog ? '加载中' : '执行过程'}
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
