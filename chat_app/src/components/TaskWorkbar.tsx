import React, { useMemo, useState } from 'react';
import { useI18n } from '../i18n/I18nProvider';
import {
  formatGuidanceAppliedTime,
  isBlockedTask,
  isDoneTask,
  isUnfinishedTask,
  groupWorkbarTasks,
  sortTasks,
} from './taskWorkbar/helpers';
import RuntimeGuidanceSection from './taskWorkbar/RuntimeGuidanceSection';
import TaskCard from './taskWorkbar/TaskCard';
import TaskHistoryDrawer from './taskWorkbar/TaskHistoryDrawer';
import TaskOutcomeModal, { type TaskOutcomeDraft } from './taskWorkbar/TaskOutcomeModal';
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
  historyOpen?: boolean;
  currentTurnId?: string | null;
  isLoading?: boolean;
  historyLoading?: boolean;
  error?: string | null;
  historyError?: string | null;
  onRefresh?: () => void;
  onOpenHistory?: () => void;
  onHistoryOpenChange?: (value: boolean) => void;
  onOpenUiPromptHistory?: () => void;
  onReviewRepair?: () => void | Promise<void>;
  reviewRepairRunning?: boolean;
  reviewRepairDisabled?: boolean;
  uiPromptHistoryCount?: number;
  uiPromptHistoryLoading?: boolean;
  onCompleteTask?: (task: TaskWorkbarItem) => void;
  onDeleteTask?: (task: TaskWorkbarItem) => void;
  onEditTask?: (task: TaskWorkbarItem) => void;
  taskModalOpen?: boolean;
  taskModalMode?: 'complete' | 'edit';
  taskModalTask?: TaskWorkbarItem | null;
  taskModalError?: string | null;
  onCloseTaskModal?: () => void;
  onSubmitTaskModal?: (draft: TaskOutcomeDraft) => void;
  actionLoadingTaskId?: string | null;
  runtimeGuidancePendingCount?: number;
  runtimeGuidanceAppliedCount?: number;
  runtimeGuidanceLastAppliedAt?: string | null;
  runtimeGuidanceItems?: RuntimeGuidanceWorkbarItem[];
}

export const TaskWorkbar: React.FC<TaskWorkbarProps> = ({
  tasks,
  historyTasks = [],
  historyOpen = false,
  currentTurnId,
  isLoading = false,
  historyLoading = false,
  error = null,
  historyError = null,
  onRefresh,
  onOpenHistory,
  onHistoryOpenChange,
  onOpenUiPromptHistory,
  onReviewRepair,
  reviewRepairRunning = false,
  reviewRepairDisabled = false,
  uiPromptHistoryCount = 0,
  uiPromptHistoryLoading = false,
  onCompleteTask,
  onDeleteTask,
  onEditTask,
  taskModalOpen = false,
  taskModalMode = 'edit',
  taskModalTask = null,
  taskModalError = null,
  onCloseTaskModal,
  onSubmitTaskModal,
  actionLoadingTaskId = null,
  runtimeGuidancePendingCount = 0,
  runtimeGuidanceAppliedCount = 0,
  runtimeGuidanceLastAppliedAt = null,
  runtimeGuidanceItems = [],
}) => {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);
  const [uncontrolledHistoryOpen, setUncontrolledHistoryOpen] = useState(false);
  const [historyFilter, setHistoryFilter] = useState<HistoryFilter>('all');
  const resolvedHistoryOpen = typeof onHistoryOpenChange === 'function'
    ? historyOpen
    : uncontrolledHistoryOpen;

  const sortedTasks = useMemo(() => sortTasks(tasks), [tasks]);
  const groupedTasks = useMemo(() => groupWorkbarTasks(sortedTasks), [sortedTasks]);
  const sortedHistoryTasks = useMemo(
    () => sortTasks(historyTasks.length > 0 ? historyTasks : sortedTasks),
    [historyTasks, sortedTasks]
  );
  const processedHistoryTasks = useMemo(
    () => sortedHistoryTasks.filter((task) => task.status === 'done'),
    [sortedHistoryTasks]
  );
  const visibleHistoryTasks = useMemo(() => {
    if (historyFilter === 'processed') {
      return processedHistoryTasks;
    }
    if (historyFilter === 'unfinished') {
      return sortedHistoryTasks.filter(isUnfinishedTask);
    }
    if (historyFilter === 'blocked') {
      return sortedHistoryTasks.filter(isBlockedTask);
    }
    if (historyFilter === 'all') {
      return sortedHistoryTasks;
    }
    return sortedHistoryTasks.filter(isDoneTask);
  }, [historyFilter, processedHistoryTasks, sortedHistoryTasks]);
  const runtimeGuidanceHint = useMemo(() => {
    if (runtimeGuidancePendingCount <= 0 && runtimeGuidanceAppliedCount <= 0) {
      return '';
    }
    const appliedAt = formatGuidanceAppliedTime(runtimeGuidanceLastAppliedAt);
    return appliedAt
      ? t('taskWorkbar.guidanceHintWithTime', {
        pending: runtimeGuidancePendingCount,
        applied: runtimeGuidanceAppliedCount,
        time: appliedAt,
      })
      : t('taskWorkbar.guidanceHint', {
        pending: runtimeGuidancePendingCount,
        applied: runtimeGuidanceAppliedCount,
      });
  }, [runtimeGuidanceAppliedCount, runtimeGuidanceLastAppliedAt, runtimeGuidancePendingCount, t]);
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

  const currentTurnTaskGroups = useMemo(() => groupWorkbarTasks(currentTurnTasks), [currentTurnTasks]);
  const currentTurnTaskSummary = currentTurnTaskGroups.current[0] || null;

  const handleOpenHistory = (filter: HistoryFilter = 'all') => {
    setHistoryFilter(filter);
    if (typeof onHistoryOpenChange === 'function') {
      onHistoryOpenChange(true);
    } else {
      setUncontrolledHistoryOpen(true);
    }
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
              <div className="text-xs font-semibold text-foreground">{t('taskWorkbar.title')}</div>
              <div className="text-[11px] text-muted-foreground">
                {t('taskWorkbar.currentTurnTasks', { count: currentTurnTasks.length })}
              </div>
              <div className="text-[11px] text-muted-foreground">
                {t('taskWorkbar.summary', {
                  current: currentTurnTaskGroups.current.length,
                  unfinished: groupedTasks.unfinished.length,
                  blocked: groupedTasks.blocked.length,
                  done: groupedTasks.done.length,
                })}
              </div>
              {runtimeGuidanceHint ? (
                <div className="text-[11px] text-muted-foreground">{runtimeGuidanceHint}</div>
              ) : null}
              {latestRuntimeGuidanceContent ? (
                <div
                  className="truncate text-[11px] text-muted-foreground"
                  title={latestRuntimeGuidanceContent}
                >
                  {t('taskWorkbar.recentGuidance', { content: latestRuntimeGuidanceContent })}
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
                {t('taskWorkbar.uiPromptHistory', {
                  count: uiPromptHistoryCount,
                  suffix: uiPromptHistoryLoading ? t('taskWorkbar.updatingSuffix') : '',
                })}
              </button>
            ) : null}
            {onReviewRepair ? (
              <button
                type="button"
                className="rounded-md border border-amber-300 bg-amber-50 px-2 py-1 text-[11px] text-amber-900 hover:bg-amber-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-100"
                onClick={() => { void onReviewRepair(); }}
                disabled={reviewRepairRunning || reviewRepairDisabled}
              >
                {reviewRepairRunning ? t('taskWorkbar.reviewRepairing') : t('taskWorkbar.reviewRepair')}
              </button>
            ) : null}
            {expanded ? (
              <>
                <button
                  type="button"
                  className="rounded-md border border-border bg-background px-2 py-1 text-[11px] text-foreground hover:bg-accent"
                  onClick={() => handleOpenHistory('all')}
                >
                  {t('taskWorkbar.history')}
                </button>
                <button
                  type="button"
                  className="rounded-md border border-border bg-background px-2 py-1 text-[11px] text-foreground hover:bg-accent"
                  onClick={() => handleOpenHistory('processed')}
                >
                  {t('taskWorkbar.processed')}
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
                {isLoading ? t('taskWorkbar.refreshing') : t('taskWorkbar.refresh')}
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
              <div className="text-[11px] text-muted-foreground">{t('taskWorkbar.loading')}</div>
            ) : null}

            {!isLoading && !currentTurnId && currentTurnTasks.length === 0 ? (
              <div className="text-[11px] text-muted-foreground">{t('taskWorkbar.noTurn')}</div>
            ) : null}

            {!isLoading && currentTurnId && currentTurnTasks.length === 0 ? (
              <div className="text-[11px] text-muted-foreground">{t('taskWorkbar.noTasksThisTurn')}</div>
            ) : null}

            {currentTurnTasks.length > 0 ? (
              <div className="space-y-2">
                <div>
                  <div className="mb-1 text-[11px] font-medium text-muted-foreground">{t('taskWorkbar.group.current')}</div>
                  <div className="flex gap-1.5 overflow-x-auto pb-1">
                    {currentTurnTaskSummary ? (
                      <TaskCard
                        key={currentTurnTaskSummary.id}
                        task={currentTurnTaskSummary}
                        compact
                        onCompleteTask={onCompleteTask}
                        onDeleteTask={onDeleteTask}
                        onEditTask={onEditTask}
                        isMutating={actionLoadingTaskId === currentTurnTaskSummary.id}
                      />
                    ) : null}
                  </div>
                </div>
                <div>
                  <div className="mb-1 text-[11px] font-medium text-muted-foreground">{t('taskWorkbar.group.unfinished')}</div>
                  <div className="flex gap-1.5 overflow-x-auto pb-1">
                    {currentTurnTaskGroups.unfinished.map((task) => (
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
                </div>
                <div>
                  <div className="mb-1 text-[11px] font-medium text-muted-foreground">{t('taskWorkbar.group.blocked')}</div>
                  <div className="flex gap-1.5 overflow-x-auto pb-1">
                    {currentTurnTaskGroups.blocked.map((task) => (
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
                </div>
                <div>
                  <div className="mb-1 text-[11px] font-medium text-muted-foreground">{t('taskWorkbar.group.done')}</div>
                  <div className="flex gap-1.5 overflow-x-auto pb-1">
                    {currentTurnTaskGroups.done.map((task) => (
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
                </div>
              </div>
            ) : null}
          </div>
        ) : null}
      </div>

      <TaskHistoryDrawer
        open={resolvedHistoryOpen}
        historyFilter={historyFilter}
        sortedHistoryTasks={sortedHistoryTasks}
        processedHistoryTasks={processedHistoryTasks}
        visibleHistoryTasks={visibleHistoryTasks}
        historyLoading={historyLoading}
        isLoading={isLoading}
        historyError={historyError}
        actionLoadingTaskId={actionLoadingTaskId}
        onClose={() => {
          if (typeof onHistoryOpenChange === 'function') {
            onHistoryOpenChange(false);
            return;
          }
          setUncontrolledHistoryOpen(false);
        }}
        onRefresh={onRefresh}
        onSetHistoryFilter={setHistoryFilter}
        onCompleteTask={onCompleteTask}
        onDeleteTask={onDeleteTask}
        onEditTask={onEditTask}
      />
      <TaskOutcomeModal
        open={taskModalOpen}
        mode={taskModalMode}
        task={taskModalTask}
        error={taskModalError}
        submitting={taskModalTask ? actionLoadingTaskId === taskModalTask.id : false}
        onClose={() => onCloseTaskModal?.()}
        onSubmit={(draft) => onSubmitTaskModal?.(draft)}
      />
    </>
  );
};

export default TaskWorkbar;
