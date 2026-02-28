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

export interface SessionSummaryWorkbarItem {
  id: string;
  summaryText: string;
  summaryModel: string;
  triggerType: string;
  sourceMessageCount: number;
  sourceEstimatedTokens: number;
  createdAt: string;
  status?: string;
  errorMessage?: string | null;
}

interface TaskWorkbarProps {
  tasks: TaskWorkbarItem[];
  historyTasks?: TaskWorkbarItem[];
  summaries?: SessionSummaryWorkbarItem[];
  hasSummaries?: boolean;
  currentTurnId?: string | null;
  isLoading?: boolean;
  historyLoading?: boolean;
  summariesLoading?: boolean;
  error?: string | null;
  historyError?: string | null;
  summariesError?: string | null;
  onRefresh?: () => void;
  onRefreshSummaries?: () => void;
  onOpenHistory?: () => void;
  onCompleteTask?: (task: TaskWorkbarItem) => void;
  onDeleteTask?: (task: TaskWorkbarItem) => void;
  onEditTask?: (task: TaskWorkbarItem) => void;
  actionLoadingTaskId?: string | null;
  onDeleteSummary?: (summary: SessionSummaryWorkbarItem) => void;
  onClearAllSummaries?: () => void;
  summaryActionLoadingId?: string | null;
  summaryBulkClearing?: boolean;
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

const SummaryCard: React.FC<{
  summary: SessionSummaryWorkbarItem;
  compact?: boolean;
  onDeleteSummary?: (summary: SessionSummaryWorkbarItem) => void;
  isMutating?: boolean;
}> = ({ summary, compact = false, onDeleteSummary, isMutating = false }) => {
  const [expandedText, setExpandedText] = useState(false);
  const cardClass = compact
    ? 'min-w-[200px] max-w-[240px] min-w-0 overflow-hidden rounded-md border border-border bg-background p-2'
    : 'min-w-0 overflow-hidden rounded-lg border border-border bg-background p-2.5';
  const canExpand = !compact && summary.summaryText.length > 280;
  const titleClass = compact
    ? 'line-clamp-3 break-words text-[11px] text-foreground'
    : expandedText
      ? 'max-h-[40vh] overflow-y-auto whitespace-pre-wrap break-words text-xs text-foreground'
      : 'line-clamp-6 break-words text-xs text-foreground';

  return (
    <div className={cardClass}>
      <div className="mb-1 flex items-center justify-between gap-2">
        <div className="truncate text-[10px] text-muted-foreground" title={summary.summaryModel}>
          {summary.summaryModel || 'unknown-model'}
        </div>
        <span className="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-200">
          {summary.triggerType || '-'}
        </span>
      </div>

      <div className={titleClass} title={summary.summaryText}>
        {summary.summaryText || '(空总结)'}
      </div>
      {canExpand ? (
        <button
          type="button"
          className="mt-1 text-[10px] text-primary hover:underline"
          onClick={() => setExpandedText((prev) => !prev)}
        >
          {expandedText ? '收起' : '展开全文'}
        </button>
      ) : null}

      <div className="mt-1 text-[10px] text-muted-foreground">
        <div>{`消息 ${summary.sourceMessageCount} · 估算 ${summary.sourceEstimatedTokens} tok`}</div>
        <div className="truncate" title={summary.createdAt}>{summary.createdAt}</div>
      </div>

      {summary.status && summary.status !== 'done' ? (
        <div className="mt-1 text-[10px] text-rose-600 dark:text-rose-300">
          {summary.errorMessage || summary.status}
        </div>
      ) : null}

      {onDeleteSummary ? (
        <div className={compact ? 'mt-1' : 'mt-2'}>
          <button
            type="button"
            className={compact
              ? 'rounded border border-border bg-background px-1.5 py-0.5 text-[10px] text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50'
              : 'rounded border border-border bg-background px-2 py-0.5 text-[11px] text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50'}
            onClick={() => onDeleteSummary(summary)}
            disabled={isMutating}
          >
            {isMutating ? '删除中...' : '删除总结'}
          </button>
        </div>
      ) : null}
    </div>
  );
};

export const TaskWorkbar: React.FC<TaskWorkbarProps> = ({
  tasks,
  historyTasks = [],
  summaries = [],
  hasSummaries = false,
  currentTurnId,
  isLoading = false,
  historyLoading = false,
  summariesLoading = false,
  error = null,
  historyError = null,
  summariesError = null,
  onRefresh,
  onRefreshSummaries,
  onOpenHistory,
  onCompleteTask,
  onDeleteTask,
  onEditTask,
  actionLoadingTaskId = null,
  onDeleteSummary,
  onClearAllSummaries,
  summaryActionLoadingId = null,
  summaryBulkClearing = false,
}) => {
  const [expanded, setExpanded] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<'tasks' | 'summaries'>('tasks');

  const showSummaryTab = hasSummaries || summaries.length > 0;

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

  React.useEffect(() => {
    if (!showSummaryTab && activeTab === 'summaries') {
      setActiveTab('tasks');
    }
  }, [activeTab, showSummaryTab]);

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
            {(onRefresh || onRefreshSummaries) ? (
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-[11px] text-foreground hover:bg-accent"
                onClick={activeTab === 'summaries' ? onRefreshSummaries : onRefresh}
                disabled={activeTab === 'summaries' ? summariesLoading : isLoading}
              >
                {(activeTab === 'summaries' ? summariesLoading : isLoading) ? '\u5237\u65b0\u4e2d...' : '\u5237\u65b0'}
              </button>
            ) : null}
            {activeTab === 'summaries' && onClearAllSummaries && summaries.length > 0 ? (
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-[11px] text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                onClick={onClearAllSummaries}
                disabled={summariesLoading || summaryBulkClearing}
              >
                {summaryBulkClearing ? '清空中...' : '清空总结'}
              </button>
            ) : null}
          </div>
        </div>

        {expanded ? (
          <div className="mt-2 border-t border-border pt-2">
            {showSummaryTab ? (
              <div className="mb-2 flex items-center gap-1">
                <button
                  type="button"
                  className={`rounded px-2 py-1 text-[11px] ${activeTab === 'tasks' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/60'}`}
                  onClick={() => setActiveTab('tasks')}
                >
                  {'任务'}
                </button>
                <button
                  type="button"
                  className={`rounded px-2 py-1 text-[11px] ${activeTab === 'summaries' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/60'}`}
                  onClick={() => setActiveTab('summaries')}
                >
                  {'会话总结'}
                </button>
              </div>
            ) : null}

            {activeTab === 'tasks' ? (
              <>
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
              </>
            ) : (
              <>
                {summariesError ? (
                  <div className="mb-2 rounded-md border border-rose-200 bg-rose-50 px-2 py-1 text-[11px] text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-200">
                    {summariesError}
                  </div>
                ) : null}
                {summariesLoading && summaries.length === 0 ? (
                  <div className="text-[11px] text-muted-foreground">{'总结加载中...'}</div>
                ) : null}
                {!summariesLoading && summaries.length === 0 ? (
                  <div className="text-[11px] text-muted-foreground">{'当前会话暂无总结。'}</div>
                ) : null}
                {summaries.length > 0 ? (
                  <div className="flex gap-1.5 overflow-x-auto pb-1">
                    {summaries.map((summary) => (
                      <SummaryCard
                        key={summary.id}
                        summary={summary}
                        compact
                        onDeleteSummary={onDeleteSummary}
                        isMutating={summaryActionLoadingId === summary.id}
                      />
                    ))}
                  </div>
                ) : null}
              </>
            )}
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
                  <div className="text-sm font-semibold text-foreground">
                    {activeTab === 'summaries' ? '会话总结' : '\u5386\u53f2\u4efb\u52a1'}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {activeTab === 'summaries'
                      ? `\u5f53\u524d\u4f1a\u8bdd\uff1a${summaries.length}`
                      : `\u5f53\u524d\u4f1a\u8bdd\uff1a${sortedHistoryTasks.length}`}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  {(onRefresh || onRefreshSummaries) ? (
                    <button
                      type="button"
                      className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent"
                      onClick={activeTab === 'summaries' ? onRefreshSummaries : onRefresh}
                      disabled={activeTab === 'summaries' ? summariesLoading : (isLoading || historyLoading)}
                    >
                      {(activeTab === 'summaries' ? summariesLoading : (isLoading || historyLoading))
                        ? '\u5237\u65b0\u4e2d...'
                        : '\u5237\u65b0'}
                    </button>
                  ) : null}
                  {activeTab === 'summaries' && onClearAllSummaries && summaries.length > 0 ? (
                    <button
                      type="button"
                      className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                      onClick={onClearAllSummaries}
                      disabled={summariesLoading || summaryBulkClearing}
                    >
                      {summaryBulkClearing ? '清空中...' : '清空总结'}
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

              {showSummaryTab ? (
                <div className="border-b border-border px-4 py-2">
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      className={`rounded px-2 py-1 text-[11px] ${activeTab === 'tasks' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/60'}`}
                      onClick={() => setActiveTab('tasks')}
                    >
                      {'任务'}
                    </button>
                    <button
                      type="button"
                      className={`rounded px-2 py-1 text-[11px] ${activeTab === 'summaries' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/60'}`}
                      onClick={() => setActiveTab('summaries')}
                    >
                      {'会话总结'}
                    </button>
                  </div>
                </div>
              ) : null}

              <div className="flex-1 overflow-y-auto px-3 py-3">
                {activeTab === 'tasks' ? (
                  <>
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
                  </>
                ) : (
                  <>
                    {summariesError ? (
                      <div className="mb-2 rounded-md border border-rose-200 bg-rose-50 px-2 py-1 text-xs text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-200">
                        {summariesError}
                      </div>
                    ) : null}

                    {summariesLoading && summaries.length === 0 ? (
                      <div className="text-xs text-muted-foreground">{'总结加载中...'}</div>
                    ) : null}

                    {!summariesLoading && summaries.length === 0 ? (
                      <div className="text-xs text-muted-foreground">{'暂无会话总结。'}</div>
                    ) : null}

                    {summaries.length > 0 ? (
                      <div className="space-y-2">
                        {summaries.map((summary) => (
                          <SummaryCard
                            key={summary.id}
                            summary={summary}
                            onDeleteSummary={onDeleteSummary}
                            isMutating={summaryActionLoadingId === summary.id}
                          />
                        ))}
                      </div>
                    ) : null}
                  </>
                )}
              </div>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
};

export default TaskWorkbar;
