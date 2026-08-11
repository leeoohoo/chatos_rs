// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useMemo, useState, type FC } from 'react';
import { Activity, ChevronDown, FileText, LoaderCircle } from 'lucide-react';
import type { MessageTaskRunnerLookupOptions } from '../../lib/api/client/messages';
import type { MessageTaskRunnerTask } from '../../lib/api/client/types';
import type { Message } from '../../types';
import { CollapsibleText } from './CollapsibleSection';
import { FieldGrid, MarkdownCard, StatusBadge } from './parts';
import { buildTaskProcessTimelineItems, TaskProcessTimeline } from './TaskProcessTimeline';
import { useMessageTasks } from './useMessageTasks';
import { formatDateTime, readString } from './utils';

interface MessageTaskInlinePanelProps {
  message: Message;
}

type InlinePanelView = 'detail' | 'process' | null;

const readStringArray = (value: unknown): string[] => (
  Array.isArray(value)
    ? value
      .map((item) => readString(item))
      .filter((item): item is string => Boolean(item))
    : []
);

const activeStatus = (value: unknown): boolean => {
  const status = readString(value)?.toLowerCase() || '';
  return ['pending', 'queued', 'running', 'processing', 'in_progress'].includes(status);
};

const terminalStatus = (value: unknown): boolean => {
  const status = readString(value)?.toLowerCase() || '';
  return ['completed', 'succeeded', 'success', 'done', 'failed', 'blocked', 'cancelled', 'canceled'].includes(status);
};

const hasTaskRunnerTaskState = (message: Message): boolean => {
  const taskRunnerAsync = message.metadata?.task_runner_async;
  if (!taskRunnerAsync) {
    return false;
  }

  const runningIds = readStringArray(taskRunnerAsync.running_task_ids);
  const queuedIds = readStringArray(taskRunnerAsync.queued_task_ids);
  const pendingIds = readStringArray(taskRunnerAsync.pending_task_ids);
  const createdIds = readStringArray(taskRunnerAsync.created_task_ids);
  const terminalIds = [
    ...readStringArray(taskRunnerAsync.terminal_task_ids),
    ...readStringArray(taskRunnerAsync.succeeded_task_ids),
    ...readStringArray(taskRunnerAsync.failed_task_ids),
    ...readStringArray(taskRunnerAsync.blocked_task_ids),
    ...readStringArray(taskRunnerAsync.cancelled_task_ids),
  ];

  return runningIds.length > 0
    || queuedIds.length > 0
    || pendingIds.length > 0
    || createdIds.length > 0
    || terminalIds.length > 0
    || activeStatus(taskRunnerAsync.overall_status)
    || activeStatus(taskRunnerAsync.status)
    || terminalStatus(taskRunnerAsync.overall_status)
    || terminalStatus(taskRunnerAsync.status)
    || Boolean(readString(taskRunnerAsync.task_id))
    || Boolean(readString(taskRunnerAsync.last_task_id));
};

const resolveLookup = (message: Message): MessageTaskRunnerLookupOptions => {
  const taskRunnerAsync = message.metadata?.task_runner_async;
  const rawSourceUserMessageId = readString(taskRunnerAsync?.source_user_message_id);
  const sourceUserMessageId = rawSourceUserMessageId?.startsWith('temp_')
    ? null
    : rawSourceUserMessageId;

  return {
    sessionId: message.sessionId,
    turnId: readString(message.metadata?.conversation_turn_id)
      || readString(taskRunnerAsync?.source_turn_id),
    sourceUserMessageId,
  };
};

const pickTask = (
  tasks: MessageTaskRunnerTask[],
  selectedTaskId: string | null,
): MessageTaskRunnerTask | null => (
  tasks.find((task) => task.id === selectedTaskId)
  || tasks[0]
  || null
);

export const MessageTaskInlinePanel: FC<MessageTaskInlinePanelProps> = ({ message }) => {
  const [expandedView, setExpandedView] = useState<InlinePanelView>(null);
  const [pendingView, setPendingView] = useState<InlinePanelView>(null);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const lookup = useMemo(() => resolveLookup(message), [message]);
  const {
    tasks,
    loading,
    error,
    detailTask,
    runDetail,
    loadingDetailId,
    loadingRunId,
    openDetail,
    openRun,
    closeDetail,
    closeRun,
  } = useMessageTasks({
    open: expandedView !== null,
    messageId: message.id,
    lookup,
  });

  const selectedTask = pickTask(tasks, selectedTaskId);
  const selectedRunId = readString(selectedTask?.last_run_id);
  const detailLoading = loadingDetailId === selectedTaskId;
  const processLoading = Boolean(selectedRunId && loadingRunId === selectedRunId);
  const detailSourceTask = detailTask?.id === selectedTaskId ? detailTask : selectedTask;
  const processSourceTask = runDetail?.task?.id === selectedTaskId ? runDetail.task : selectedTask;
  const processTasks = runDetail?.task?.id === selectedTaskId && Array.isArray(runDetail.process_tasks)
    ? runDetail.process_tasks
    : [];
  const processTimelineItems = buildTaskProcessTimelineItems(
    readString(processSourceTask?.process_log),
    processTasks,
    readString(processSourceTask?.status),
  );
  const prerequisiteCount = Array.isArray(detailSourceTask?.prerequisite_task_ids)
    ? detailSourceTask.prerequisite_task_ids.length
    : 0;
  const objective = readString(detailSourceTask?.objective || selectedTask?.objective);
  const description = readString(detailSourceTask?.description || selectedTask?.description);
  const hasTaskState = useMemo(() => hasTaskRunnerTaskState(message), [message]);
  const shouldRenderPanel = hasTaskState || expandedView !== null || loading || tasks.length > 0 || Boolean(error);

  useEffect(() => {
    if (selectedTaskId || tasks.length === 0) {
      return;
    }
    setSelectedTaskId(tasks[0].id);
  }, [selectedTaskId, tasks]);

  useEffect(() => {
    if (!expandedView || !pendingView) {
      return;
    }
    const nextTask = pickTask(tasks, selectedTaskId);
    if (!nextTask) {
      return;
    }
    setSelectedTaskId(nextTask.id);
    if (pendingView === 'detail') {
      void openDetail(nextTask);
    } else {
      void openRun(nextTask);
    }
    setPendingView(null);
  }, [expandedView, openDetail, openRun, pendingView, selectedTaskId, tasks]);

  useEffect(() => {
    setExpandedView(null);
    setPendingView(null);
    setSelectedTaskId(null);
    closeDetail();
    closeRun();
  }, [closeDetail, closeRun, message.id]);

  const handleOpenView = (view: Exclude<InlinePanelView, null>) => {
    if (expandedView === view) {
      setExpandedView(null);
      setPendingView(null);
      closeDetail();
      closeRun();
      return;
    }
    setExpandedView(view);
    const nextTask = pickTask(tasks, selectedTaskId);
    if (!nextTask) {
      setPendingView(view);
      return;
    }
    setSelectedTaskId(nextTask.id);
    if (view === 'detail') {
      void openDetail(nextTask);
    } else {
      void openRun(nextTask);
    }
    setPendingView(null);
  };

  const handleTaskChange = (taskId: string) => {
    setSelectedTaskId(taskId);
    const nextTask = tasks.find((task) => task.id === taskId);
    if (!nextTask || !expandedView) {
      return;
    }
    if (expandedView === 'detail') {
      void openDetail(nextTask);
    } else {
      void openRun(nextTask);
    }
  };

  const viewButtonClass = (active: boolean) => (
    active
      ? 'text-primary'
      : 'text-muted-foreground hover:text-foreground'
  );

  if (!shouldRenderPanel) {
    return null;
  }

  return (
    <div className="rounded-lg border border-border/70 bg-muted/20 px-3 py-2.5">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 text-xs">
        <button
          type="button"
          className={`inline-flex items-center gap-1 font-medium transition-colors ${viewButtonClass(expandedView === 'process')}`}
          onClick={() => handleOpenView('process')}
        >
          {processLoading ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Activity className="h-3.5 w-3.5" />}
          查看过程
          <ChevronDown className={`h-3.5 w-3.5 transition-transform ${expandedView === 'process' ? 'rotate-180' : ''}`} />
        </button>
        <button
          type="button"
          className={`inline-flex items-center gap-1 font-medium transition-colors ${viewButtonClass(expandedView === 'detail')}`}
          onClick={() => handleOpenView('detail')}
        >
          {detailLoading ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <FileText className="h-3.5 w-3.5" />}
          查看详情
          <ChevronDown className={`h-3.5 w-3.5 transition-transform ${expandedView === 'detail' ? 'rotate-180' : ''}`} />
        </button>
        {selectedTask ? (
          <span className="text-muted-foreground">
            当前任务：{selectedTask.title || selectedTask.id}
          </span>
        ) : null}
      </div>

      {expandedView ? (
        <div className="mt-3 border-t border-border/70 pt-3">
          {tasks.length > 1 ? (
            <label className="mb-3 flex items-center gap-2 text-xs text-muted-foreground">
              <span className="shrink-0">查看任务</span>
              <select
                className="min-w-0 flex-1 rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground"
                value={selectedTaskId || tasks[0].id}
                onChange={(event) => handleTaskChange(event.target.value)}
              >
                {tasks.map((task, index) => (
                  <option key={task.id} value={task.id}>
                    {index + 1}. {task.title || task.id}
                  </option>
                ))}
              </select>
            </label>
          ) : null}

          {loading && tasks.length === 0 ? (
            <div className="flex items-center gap-2 rounded-md border border-border bg-background px-3 py-3 text-sm text-muted-foreground">
              <LoaderCircle className="h-4 w-4 animate-spin" />
              正在加载任务内容...
            </div>
          ) : null}

          {!loading && tasks.length === 0 && !error ? (
            <div className="rounded-md border border-dashed border-border bg-background px-3 py-3 text-sm text-muted-foreground">
              当前消息还没有可展开查看的任务内容。
            </div>
          ) : null}

          {error ? (
            <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-3 text-sm text-destructive">
              {error}
            </div>
          ) : null}

          {selectedTask && expandedView === 'process' && tasks.length > 0 ? (
            <div className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm font-semibold text-foreground">
                  {processSourceTask?.title || selectedTask.id}
                </span>
                <StatusBadge status={processSourceTask?.status || selectedTask.status} />
              </div>
              <TaskProcessTimeline items={processTimelineItems} />
            </div>
          ) : null}

          {selectedTask && expandedView === 'detail' && tasks.length > 0 ? (
            <div className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm font-semibold text-foreground">
                  {detailSourceTask?.title || selectedTask.id}
                </span>
                <StatusBadge status={detailSourceTask?.status || selectedTask.status} />
              </div>

              <FieldGrid
                items={[
                  ['任务 ID', detailSourceTask?.id || selectedTask.id],
                  ['最近运行', detailSourceTask?.last_run?.status || selectedTask.last_run?.status],
                  ['前置依赖', prerequisiteCount],
                  ['更新时间', formatDateTime(detailSourceTask?.updated_at || selectedTask.updated_at)],
                ]}
              />

              {readString(detailSourceTask?.result_summary) ? (
                <section>
                  <div className="mb-1.5 text-xs font-medium text-muted-foreground">执行结果</div>
                  <MarkdownCard content={detailSourceTask?.result_summary} />
                </section>
              ) : null}

              {objective ? (
                <section>
                  <div className="mb-1.5 text-xs font-medium text-muted-foreground">目标</div>
                  <CollapsibleText value={objective} />
                </section>
              ) : null}

              {description ? (
                <section>
                  <div className="mb-1.5 text-xs font-medium text-muted-foreground">描述</div>
                  <CollapsibleText value={description} />
                </section>
              ) : null}
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
};
