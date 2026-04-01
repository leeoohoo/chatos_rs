import React, { useMemo, useState } from 'react';
import {
  formatGuidanceAppliedTime,
  sortTasks,
} from './taskWorkbar/helpers';
import RuntimeGuidanceSection from './taskWorkbar/RuntimeGuidanceSection';
import TaskCard from './taskWorkbar/TaskCard';
import TaskHistoryDrawer from './taskWorkbar/TaskHistoryDrawer';
import type {
  HistoryFilter,
  RuntimeGuidanceWorkbarItem,
  TaskWorkbarItem,
} from './taskWorkbar/types';

export type {
  RuntimeGuidanceWorkbarItem,
  SessionSummaryWorkbarItem,
  TaskWorkbarItem,
} from './taskWorkbar/types';

interface TaskWorkbarProps {
  tasks: TaskWorkbarItem[];
  historyTasks?: TaskWorkbarItem[];
  currentTurnId?: string | null;
  isLoading?: boolean;
  historyLoading?: boolean;
  error?: string | null;
  historyError?: string | null;
  onRefresh?: () => void;
  onOpenHistory?: () => void;
  onOpenUiPromptHistory?: () => void;
  uiPromptHistoryCount?: number;
  uiPromptHistoryLoading?: boolean;
  onCompleteTask?: (task: TaskWorkbarItem) => void;
  onDeleteTask?: (task: TaskWorkbarItem) => void;
  onEditTask?: (task: TaskWorkbarItem) => void;
  actionLoadingTaskId?: string | null;
  runtimeGuidancePendingCount?: number;
  runtimeGuidanceAppliedCount?: number;
  runtimeGuidanceLastAppliedAt?: string | null;
  runtimeGuidanceItems?: RuntimeGuidanceWorkbarItem[];
}

export const TaskWorkbar: React.FC<TaskWorkbarProps> = ({
  tasks,
  historyTasks = [],
  currentTurnId,
  isLoading = false,
  historyLoading = false,
  error = null,
  historyError = null,
  onRefresh,
  onOpenHistory,
  onOpenUiPromptHistory,
  uiPromptHistoryCount = 0,
  uiPromptHistoryLoading = false,
  onCompleteTask,
  onDeleteTask,
  onEditTask,
  actionLoadingTaskId = null,
  runtimeGuidancePendingCount = 0,
  runtimeGuidanceAppliedCount = 0,
  runtimeGuidanceLastAppliedAt = null,
  runtimeGuidanceItems = [],
}) => {
  const [expanded, setExpanded] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyFilter, setHistoryFilter] = useState<HistoryFilter>('all');

  const sortedTasks = useMemo(() => sortTasks(tasks), [tasks]);
  const sortedHistoryTasks = useMemo(
    () => sortTasks(historyTasks.length > 0 ? historyTasks : sortedTasks),
    [historyTasks, sortedTasks]
  );
  const processedHistoryTasks = useMemo(
    () => sortedHistoryTasks.filter((task) => task.status === 'done'),
    [sortedHistoryTasks]
  );
  const visibleHistoryTasks = historyFilter === 'processed'
    ? processedHistoryTasks
    : sortedHistoryTasks;
  const runtimeGuidanceHint = useMemo(() => {
    if (runtimeGuidancePendingCount <= 0 && runtimeGuidanceAppliedCount <= 0) {
      return '';
    }
    const appliedAt = formatGuidanceAppliedTime(runtimeGuidanceLastAppliedAt);
    return appliedAt
      ? `引导待应用: ${runtimeGuidancePendingCount} · 已应用: ${runtimeGuidanceAppliedCount} · 最近应用: ${appliedAt}`
      : `引导待应用: ${runtimeGuidancePendingCount} · 已应用: ${runtimeGuidanceAppliedCount}`;
  }, [runtimeGuidanceAppliedCount, runtimeGuidanceLastAppliedAt, runtimeGuidancePendingCount]);
  const visibleRuntimeGuidanceItems = useMemo(() => {
    const normalizedCurrentTurnId = typeof currentTurnId === 'string' ? currentTurnId.trim() : '';
    const sortedItems = [...runtimeGuidanceItems].sort((a, b) => {
      const left = Date.parse(a.createdAt) || 0;
      const right = Date.parse(b.createdAt) || 0;
      return right - left;
    });
    if (sortedItems.length === 0) {
      return [];
    }
    if (!normalizedCurrentTurnId) {
      return sortedItems.slice(0, 3);
    }
    const scoped = sortedItems.filter((item) => (item.turnId || '').trim() === normalizedCurrentTurnId);
    return (scoped.length > 0 ? scoped : sortedItems).slice(0, 3);
  }, [currentTurnId, runtimeGuidanceItems]);
  const latestRuntimeGuidanceContent = useMemo(() => {
    const latest = visibleRuntimeGuidanceItems.find((item) => item.content && item.content.trim().length > 0);
    return latest ? latest.content.trim() : '';
  }, [visibleRuntimeGuidanceItems]);

  const currentTurnTasks = useMemo(() => {
    const normalizedCurrentTurnId = typeof currentTurnId === 'string' ? currentTurnId.trim() : '';

    if (normalizedCurrentTurnId) {
      const scoped = sortedTasks.filter((task) => task.conversationTurnId.trim() === normalizedCurrentTurnId);
      if (scoped.length > 0) {
        return scoped;
      }
    }

    const fallbackSource = sortedHistoryTasks.length > 0 ? sortedHistoryTasks : sortedTasks;
    const latestTurnId = fallbackSource.find((task) => task.conversationTurnId?.trim())?.conversationTurnId?.trim() || '';
    if (!latestTurnId) {
      return [];
    }

    return fallbackSource.filter((task) => task.conversationTurnId.trim() === latestTurnId);
  }, [currentTurnId, sortedHistoryTasks, sortedTasks]);

  const handleOpenHistory = (filter: HistoryFilter = 'all') => {
    setHistoryFilter(filter);
    setHistoryOpen(true);
    onOpenHistory?.();
  };

  return (
    <>
      <div className="mx-2 mt-2 rounded-lg border border-border bg-card/70 px-3 py-2">
        <div className="flex items-center justify-between gap-2">
          <button
            type="button"
            className="flex min-w-0 items-center gap-2 text-left"
            onClick={() => setExpanded((prev) => !prev)}
          >
            <svg
              className={`h-3.5 w-3.5 text-muted-foreground transition-transform ${expanded ? 'rotate-90' : ''}`}
              viewBox="0 0 20 20"
              fill="currentColor"
            >
              <path d="M7 5l6 5-6 5V5z" />
            </svg>
            <div className="min-w-0">
              <div className="text-xs font-semibold text-foreground">Workbar</div>
              <div className="text-[11px] text-muted-foreground">{`\u5f53\u524d\u8f6e\u4efb\u52a1\uff1a${currentTurnTasks.length}`}</div>
              {runtimeGuidanceHint ? (
                <div className="text-[11px] text-muted-foreground">{runtimeGuidanceHint}</div>
              ) : null}
              {latestRuntimeGuidanceContent ? (
                <div
                  className="truncate text-[11px] text-muted-foreground"
                  title={latestRuntimeGuidanceContent}
                >
                  {`最近引导：${latestRuntimeGuidanceContent}`}
                </div>
              ) : null}
            </div>
          </button>

          <div className="flex items-center gap-2">
            {onOpenUiPromptHistory ? (
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-[11px] text-foreground hover:bg-accent"
                onClick={onOpenUiPromptHistory}
              >
                {`交互确认记录 ${uiPromptHistoryCount}${uiPromptHistoryLoading ? ' · 更新中' : ''}`}
              </button>
            ) : null}
            {expanded ? (
              <>
                <button
                  type="button"
                  className="rounded-md border border-border bg-background px-2 py-1 text-[11px] text-foreground hover:bg-accent"
                  onClick={() => handleOpenHistory('all')}
                >
                  {'历史任务'}
                </button>
                <button
                  type="button"
                  className="rounded-md border border-border bg-background px-2 py-1 text-[11px] text-foreground hover:bg-accent"
                  onClick={() => handleOpenHistory('processed')}
                >
                  {'已处理'}
                </button>
              </>
            ) : null}
            {onRefresh ? (
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-[11px] text-foreground hover:bg-accent"
                onClick={onRefresh}
                disabled={isLoading}
              >
                {isLoading ? '\u5237\u65b0\u4e2d...' : '\u5237\u65b0'}
              </button>
            ) : null}
          </div>
        </div>

        {expanded ? (
          <div className="mt-2 border-t border-border pt-2">
            {error ? (
              <div className="mb-2 rounded-md border border-rose-200 bg-rose-50 px-2 py-1 text-[11px] text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-200">
                {error}
              </div>
            ) : null}

            <RuntimeGuidanceSection items={visibleRuntimeGuidanceItems} />

            {isLoading && currentTurnTasks.length === 0 ? (
              <div className="text-[11px] text-muted-foreground">{'\u4efb\u52a1\u52a0\u8f7d\u4e2d...'}</div>
            ) : null}

            {!isLoading && !currentTurnId && currentTurnTasks.length === 0 ? (
              <div className="text-[11px] text-muted-foreground">{'\u5f53\u524d\u6682\u65e0\u8f6e\u6b21\u3002'}</div>
            ) : null}

            {!isLoading && currentTurnId && currentTurnTasks.length === 0 ? (
              <div className="text-[11px] text-muted-foreground">{'\u672c\u8f6e\u6682\u65e0\u4efb\u52a1\u3002'}</div>
            ) : null}

            {currentTurnTasks.length > 0 ? (
              <div className="flex gap-1.5 overflow-x-auto pb-1">
                {currentTurnTasks.map((task) => (
                  <TaskCard
                    key={task.id}
                    task={task}
                    compact
                    onCompleteTask={onCompleteTask}
                    onDeleteTask={onDeleteTask}
                    onEditTask={onEditTask}
                    isMutating={actionLoadingTaskId === task.id}
                  />
                ))}
              </div>
            ) : null}
          </div>
        ) : null}
      </div>

      <TaskHistoryDrawer
        open={historyOpen}
        historyFilter={historyFilter}
        sortedHistoryTasks={sortedHistoryTasks}
        processedHistoryTasks={processedHistoryTasks}
        visibleHistoryTasks={visibleHistoryTasks}
        historyLoading={historyLoading}
        isLoading={isLoading}
        historyError={historyError}
        actionLoadingTaskId={actionLoadingTaskId}
        onClose={() => setHistoryOpen(false)}
        onRefresh={onRefresh}
        onSetHistoryFilter={setHistoryFilter}
        onCompleteTask={onCompleteTask}
        onDeleteTask={onDeleteTask}
        onEditTask={onEditTask}
      />
    </>
  );
};

export default TaskWorkbar;
