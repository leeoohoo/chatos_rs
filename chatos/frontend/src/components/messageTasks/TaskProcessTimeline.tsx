// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import { Check, Circle, CircleAlert, Clock3, LoaderCircle } from 'lucide-react';
import type { MessageTaskRunnerTask } from '../../lib/api/client/types';
import { cn } from '../../lib/utils';
import { formatDateTime, isRecord, readString } from './utils';

export interface TaskProcessTimelineItem {
  id: string;
  title: string;
  description: string;
  occurredAt: string | null;
  status: string | null;
  source: 'task' | 'process_task';
}

interface ParsedProcessLogEntry {
  title: string;
  description: string;
  occurredAt: string | null;
}

const PROCESS_LOG_HEADER = /^\[([^\]]+)\]\s*(.*)$/;

export const parseTaskProcessLog = (value?: string | null): ParsedProcessLogEntry[] => {
  const text = readString(value);
  if (!text) {
    return [];
  }

  const entries: ParsedProcessLogEntry[] = [];
  let current: ParsedProcessLogEntry | null = null;

  const flush = () => {
    if (!current) {
      return;
    }
    const description = current.description.trim();
    entries.push({
      ...current,
      description: description || '暂无过程说明',
    });
    current = null;
  };

  text.replace(/\r\n?/g, '\n').split('\n').forEach((line) => {
    const header = line.match(PROCESS_LOG_HEADER);
    if (header) {
      flush();
      current = {
        occurredAt: readString(header[1]),
        title: readString(header[2]) || '过程记录',
        description: '',
      };
      return;
    }

    if (!current) {
      if (!line.trim()) {
        return;
      }
      current = {
        occurredAt: null,
        title: '过程记录',
        description: line,
      };
      return;
    }

    current.description = current.description
      ? `${current.description}\n${line}`
      : line;
  });

  flush();
  return entries;
};

const processTaskDescription = (task: MessageTaskRunnerTask): string | null => {
  const toolState = isRecord(task.task_tool_state) ? task.task_tool_state : null;
  return readString(task.process_log)
    || readString(toolState?.resume_hint)
    || readString(task.result_summary);
};

const timestampValue = (value: string | null): number | null => {
  if (!value) {
    return null;
  }
  const timestamp = new Date(value).getTime();
  return Number.isNaN(timestamp) ? null : timestamp;
};

export const buildTaskProcessTimelineItems = (
  processLog: string | null,
  processTasks: MessageTaskRunnerTask[],
  taskStatus?: string | null,
): TaskProcessTimelineItem[] => {
  const items: TaskProcessTimelineItem[] = parseTaskProcessLog(processLog).map((entry, index) => ({
    id: `task-${entry.occurredAt || 'record'}-${index}`,
    title: entry.title,
    description: entry.description,
    occurredAt: entry.occurredAt,
    status: null,
    source: 'task',
  }));

  processTasks.forEach((task, taskIndex) => {
    const taskTitle = readString(task.title) || task.id;
    const taskEntries = parseTaskProcessLog(processTaskDescription(task));
    const entries = taskEntries.length > 0
      ? taskEntries
      : [{
        title: taskTitle,
        description: '暂无过程说明',
        occurredAt: readString(task.updated_at) || readString(task.created_at),
      }];

    entries.forEach((entry, entryIndex) => {
      items.push({
        id: `process-task-${task.id}-${taskIndex}-${entryIndex}`,
        title: entry.title === '过程记录' ? taskTitle : `${taskTitle} · ${entry.title}`,
        description: entry.description,
        occurredAt: entry.occurredAt
          || readString(task.updated_at)
          || readString(task.created_at),
        status: readString(task.status),
        source: 'process_task',
      });
    });
  });

  const ordered = items
    .map((item, index) => ({ item, index, timestamp: timestampValue(item.occurredAt) }))
    .sort((left, right) => {
      if (left.timestamp === null && right.timestamp === null) return left.index - right.index;
      if (left.timestamp === null) return 1;
      if (right.timestamp === null) return -1;
      return left.timestamp - right.timestamp || left.index - right.index;
    })
    .map(({ item }) => item);

  return ordered.map((item, index) => {
    if (item.status) {
      return item;
    }
    return {
      ...item,
      status: index === ordered.length - 1 ? readString(taskStatus) || 'succeeded' : 'succeeded',
    };
  });
};

export const TaskProcessTimeline: FC<{ items: TaskProcessTimelineItem[] }> = ({ items }) => {
  if (!items.length) {
    return (
      <div className="rounded-xl border border-dashed border-border bg-muted/20 px-4 py-10 text-center">
        <p className="text-sm font-medium text-foreground">暂无执行过程</p>
        <p className="mt-1 text-xs text-muted-foreground">任务写入关键执行节点后会按时间展示在这里。</p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-border bg-muted/20 px-4 py-3">
        <div>
          <p className="text-sm font-semibold text-foreground">执行时间线</p>
          <p className="mt-0.5 text-xs text-muted-foreground">按时间顺序展示任务执行期间记录的关键节点</p>
        </div>
        <span className="rounded-full border border-border bg-background px-2.5 py-1 text-xs font-medium text-muted-foreground">
          {items.length} 个节点
        </span>
      </div>

      <ol className="relative space-y-0 before:absolute before:bottom-5 before:left-[17px] before:top-5 before:w-px before:bg-border">
        {items.map((item, index) => {
          const tone = processStatusTone(item.status);
          const Icon = processStatusIcon(item.status);
          return (
            <li key={item.id} className="relative pb-4 pl-12 last:pb-0">
              <span
                className={cn(
                  'absolute left-0 top-3 z-10 grid h-9 w-9 place-items-center rounded-full border-4 border-card',
                  tone.dot,
                )}
                aria-hidden="true"
              >
                <Icon className={cn('h-3.5 w-3.5', tone.icon, isRunningStatus(item.status) && 'animate-spin')} />
              </span>

              <article className={cn('rounded-xl border bg-background p-4 shadow-sm', tone.card)}>
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <h3 className="break-words text-sm font-semibold text-foreground">{item.title}</h3>
                      <span className={cn('rounded-full border px-2 py-0.5 text-[11px] font-medium', tone.badge)}>
                        {processStatusLabel(item.status)}
                      </span>
                    </div>
                    <div className="mt-1.5 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                      <span>{item.source === 'task' ? '任务记录' : '执行节点'}</span>
                      <span aria-hidden="true">·</span>
                      <span>节点 {index + 1}</span>
                    </div>
                  </div>
                  {item.occurredAt ? (
                    <span className="inline-flex shrink-0 items-center gap-1.5 rounded-full bg-muted px-2.5 py-1 text-[11px] text-muted-foreground">
                      <Clock3 className="h-3 w-3" />
                      {formatDateTime(item.occurredAt)}
                    </span>
                  ) : null}
                </div>

                <div className="mt-3 border-t border-border/70 pt-3">
                  <p className="whitespace-pre-wrap break-words text-sm leading-6 text-muted-foreground">
                    {item.description}
                  </p>
                </div>
              </article>
            </li>
          );
        })}
      </ol>
    </div>
  );
};

const normalizedStatus = (status?: string | null): string => readString(status)?.toLowerCase() || '';

const isRunningStatus = (status?: string | null): boolean => (
  ['doing', 'processing', 'running'].includes(normalizedStatus(status))
);

const processStatusLabel = (status?: string | null): string => {
  const normalized = normalizedStatus(status);
  if (['completed', 'success', 'succeeded'].includes(normalized)) return '已完成';
  if (['doing', 'processing', 'running'].includes(normalized)) return '进行中';
  if (['failed', 'error'].includes(normalized)) return '失败';
  if (normalized === 'blocked') return '阻塞';
  if (normalized === 'cancelled') return '已取消';
  if (['draft', 'pending', 'queued', 'ready', 'waiting_prerequisite'].includes(normalized)) return '待执行';
  return '已记录';
};

const processStatusIcon = (status?: string | null) => {
  const normalized = normalizedStatus(status);
  if (['completed', 'success', 'succeeded'].includes(normalized)) return Check;
  if (['doing', 'processing', 'running'].includes(normalized)) return LoaderCircle;
  if (['blocked', 'error', 'failed'].includes(normalized)) return CircleAlert;
  return Circle;
};

const processStatusTone = (status?: string | null) => {
  const normalized = normalizedStatus(status);
  if (['completed', 'success', 'succeeded'].includes(normalized)) {
    return {
      dot: 'bg-emerald-100 dark:bg-emerald-950',
      icon: 'text-emerald-600 dark:text-emerald-300',
      card: 'border-emerald-200/80 dark:border-emerald-900/80',
      badge: 'border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950 dark:text-emerald-300',
    };
  }
  if (['doing', 'processing', 'running'].includes(normalized)) {
    return {
      dot: 'bg-sky-100 dark:bg-sky-950',
      icon: 'text-sky-600 dark:text-sky-300',
      card: 'border-sky-300 ring-2 ring-sky-100/80 dark:border-sky-800 dark:ring-sky-950/60',
      badge: 'border-sky-200 bg-sky-50 text-sky-700 dark:border-sky-900 dark:bg-sky-950 dark:text-sky-300',
    };
  }
  if (['error', 'failed'].includes(normalized)) {
    return {
      dot: 'bg-red-100 dark:bg-red-950',
      icon: 'text-red-600 dark:text-red-300',
      card: 'border-red-200/80 dark:border-red-900/80',
      badge: 'border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300',
    };
  }
  if (normalized === 'blocked') {
    return {
      dot: 'bg-amber-100 dark:bg-amber-950',
      icon: 'text-amber-600 dark:text-amber-300',
      card: 'border-amber-200/80 dark:border-amber-900/80',
      badge: 'border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-300',
    };
  }
  return {
    dot: 'bg-muted',
    icon: 'text-muted-foreground',
    card: 'border-border',
    badge: 'border-border bg-muted text-muted-foreground',
  };
};
