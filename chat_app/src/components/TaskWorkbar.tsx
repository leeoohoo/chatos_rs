import React, { useMemo, useState } from 'react';

export interface TaskWorkbarItem {
  id: string;
  title: string;
  details: string;
  status: 'todo' | 'doing' | 'blocked' | 'done';
  priority: 'high' | 'medium' | 'low';
  conversationTurnId: string;
  createdAt: string;
  dueAt?: string | null;
  tags: string[];
}

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
  onCompleteTask?: (task: TaskWorkbarItem) => void;
  onDeleteTask?: (task: TaskWorkbarItem) => void;
  onEditTask?: (task: TaskWorkbarItem) => void;
  actionLoadingTaskId?: string | null;
}

const statusStyles: Record<TaskWorkbarItem['status'], string> = {
  todo: 'bg-slate-200 text-slate-700 dark:bg-slate-700 dark:text-slate-100',
  doing: 'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-200',
  blocked: 'bg-rose-100 text-rose-700 dark:bg-rose-900/40 dark:text-rose-200',
  done: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-200',
};

const priorityStyles: Record<TaskWorkbarItem['priority'], string> = {
  high: 'text-rose-600 dark:text-rose-300',
  medium: 'text-amber-600 dark:text-amber-300',
  low: 'text-slate-600 dark:text-slate-300',
};

const statusText: Record<TaskWorkbarItem['status'], string> = {
  todo: '\u5f85\u529e',
  doing: '\u8fdb\u884c\u4e2d',
  blocked: '\u963b\u585e',
  done: '\u5df2\u5b8c\u6210',
};

const priorityText: Record<TaskWorkbarItem['priority'], string> = {
  high: '\u9ad8',
  medium: '\u4e2d',
  low: '\u4f4e',
};

const sortTasks = (items: TaskWorkbarItem[]) => {
  return [...items].sort((a, b) => {
    const left = Date.parse(a.createdAt) || 0;
    const right = Date.parse(b.createdAt) || 0;
    return right - left;
  });
};

const TaskCard: React.FC<{
  task: TaskWorkbarItem;
  compact?: boolean;
  onCompleteTask?: (task: TaskWorkbarItem) => void;
  onDeleteTask?: (task: TaskWorkbarItem) => void;
  onEditTask?: (task: TaskWorkbarItem) => void;
  isMutating?: boolean;
}> = ({
  task,
  compact = false,
  onCompleteTask,
  onDeleteTask,
  onEditTask,
  isMutating = false,
}) => {
  const cardClass = compact
    ? 'min-w-[160px] max-w-[190px] min-w-0 overflow-hidden rounded-md border border-border bg-background p-2'
    : 'min-w-0 overflow-hidden rounded-lg border border-border bg-background p-2.5';

  const titleClass = compact
    ? 'min-w-0 line-clamp-2 break-words text-xs font-medium text-foreground'
    : 'min-w-0 line-clamp-2 break-words text-sm font-medium text-foreground';

  const detailsClass = compact
    ? 'mb-1 line-clamp-1 break-all text-[11px] text-muted-foreground'
    : 'mb-1 line-clamp-2 break-all text-xs text-muted-foreground';

  const metaClass = compact ? 'text-[10px] text-muted-foreground' : 'text-[11px] text-muted-foreground';
  const actionClass = compact
    ? 'rounded border border-border bg-background px-1.5 py-0.5 text-[10px] text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50'
    : 'rounded border border-border bg-background px-2 py-0.5 text-[11px] text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50';

  return (
    <div className={cardClass}>
      <div className="mb-1 flex min-w-0 items-start justify-between gap-2">
        <div className={titleClass}>{task.title}</div>
        <span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${statusStyles[task.status]}`}>
          {statusText[task.status]}
        </span>
      </div>

      {task.details ? <div className={detailsClass}>{task.details}</div> : null}

      <div className={metaClass}>
        <div>
          <span className={priorityStyles[task.priority]}>{'\u4f18\u5148\u7ea7'} {priorityText[task.priority]}</span>
        </div>
        <div className="truncate" title={task.conversationTurnId}>
          {'\u8f6e\u6b21'} {task.conversationTurnId}
        </div>
      </div>

      {(onCompleteTask || onEditTask || onDeleteTask) ? (
        <div className={compact ? 'mt-1 flex items-center gap-1' : 'mt-2 flex items-center gap-1'}>
          {onCompleteTask && task.status !== 'done' ? (
            <button type="button" className={actionClass} onClick={() => onCompleteTask(task)} disabled={isMutating}>
              {'完成'}
            </button>
          ) : null}
          {onEditTask ? (
            <button type="button" className={actionClass} onClick={() => onEditTask(task)} disabled={isMutating}>
              {'编辑'}
            </button>
          ) : null}
          {onDeleteTask ? (
            <button type="button" className={actionClass} onClick={() => onDeleteTask(task)} disabled={isMutating}>
              {'删除'}
            </button>
          ) : null}
          {isMutating ? (
            <span className={compact ? 'text-[10px] text-muted-foreground' : 'text-[11px] text-muted-foreground'}>
              {'处理中...'}
            </span>
          ) : null}
        </div>
      ) : null}

      {task.dueAt ? (
        <div className={compact ? 'mt-1 truncate text-[10px] text-muted-foreground' : 'mt-1 truncate text-[11px] text-muted-foreground'} title={task.dueAt}>
          {'截止'} {task.dueAt}
        </div>
      ) : null}
    </div>
  );
};

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
  onCompleteTask,
  onDeleteTask,
  onEditTask,
  actionLoadingTaskId = null,
}) => {
  const [expanded, setExpanded] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);

  const sortedTasks = useMemo(() => sortTasks(tasks), [tasks]);
  const sortedHistoryTasks = useMemo(
    () => sortTasks(historyTasks.length > 0 ? historyTasks : sortedTasks),
    [historyTasks, sortedTasks]
  );

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

  const handleOpenHistory = () => {
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
            </div>
          </button>

          <div className="flex items-center gap-2">
            {expanded ? (
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-[11px] text-foreground hover:bg-accent"
                onClick={handleOpenHistory}
              >
                {'\u5c55\u793a\u66f4\u591a'}
              </button>
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

      {historyOpen ? (
        <div className="fixed inset-0 z-50">
          <button
            type="button"
            aria-label={'\u5173\u95ed\u5386\u53f2\u4efb\u52a1\u62bd\u5c49'}
            className="absolute inset-0 bg-black/35"
            onClick={() => setHistoryOpen(false)}
          />
          <div className="absolute right-0 top-0 h-full w-full max-w-md border-l border-border bg-card shadow-xl">
            <div className="flex h-full flex-col">
              <div className="flex items-center justify-between border-b border-border px-4 py-3">
                <div>
                  <div className="text-sm font-semibold text-foreground">{'\u5386\u53f2\u4efb\u52a1'}</div>
                  <div className="text-xs text-muted-foreground">{`\u5f53\u524d\u4f1a\u8bdd\uff1a${sortedHistoryTasks.length}`}</div>
                </div>
                <div className="flex items-center gap-2">
                  {onRefresh ? (
                    <button
                      type="button"
                      className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent"
                      onClick={onRefresh}
                      disabled={isLoading || historyLoading}
                    >
                      {isLoading || historyLoading ? '\u5237\u65b0\u4e2d...' : '\u5237\u65b0'}
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent"
                    onClick={() => setHistoryOpen(false)}
                  >
                    {'\u5173\u95ed'}
                  </button>
                </div>
              </div>

              <div className="flex-1 overflow-y-auto px-3 py-3">
                {historyError ? (
                  <div className="mb-2 rounded-md border border-rose-200 bg-rose-50 px-2 py-1 text-xs text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-200">
                    {historyError}
                  </div>
                ) : null}

                {historyLoading || (isLoading && sortedHistoryTasks.length === 0) ? (
                  <div className="text-xs text-muted-foreground">{'\u5386\u53f2\u4efb\u52a1\u52a0\u8f7d\u4e2d...'}</div>
                ) : null}

                {!historyLoading && sortedHistoryTasks.length === 0 ? (
                  <div className="text-xs text-muted-foreground">{'\u6682\u65e0\u5386\u53f2\u4efb\u52a1\u3002'}</div>
                ) : null}

                {sortedHistoryTasks.length > 0 ? (
                  <div className="space-y-2">
                    {sortedHistoryTasks.map((task) => (
                      <TaskCard
                        key={task.id}
                        task={task}
                        onCompleteTask={onCompleteTask}
                        onDeleteTask={onDeleteTask}
                        onEditTask={onEditTask}
                        isMutating={actionLoadingTaskId === task.id}
                      />
                    ))}
                  </div>
                ) : null}
              </div>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
};

export default TaskWorkbar;
