import TaskCard from './TaskCard';
import type { HistoryFilter, TaskWorkbarItem } from './types';

interface TaskHistoryDrawerProps {
  open: boolean;
  historyFilter: HistoryFilter;
  sortedHistoryTasks: TaskWorkbarItem[];
  processedHistoryTasks: TaskWorkbarItem[];
  visibleHistoryTasks: TaskWorkbarItem[];
  historyLoading?: boolean;
  isLoading?: boolean;
  historyError?: string | null;
  actionLoadingTaskId?: string | null;
  onClose: () => void;
  onRefresh?: () => void;
  onSetHistoryFilter: (filter: HistoryFilter) => void;
  onCompleteTask?: (task: TaskWorkbarItem) => void;
  onDeleteTask?: (task: TaskWorkbarItem) => void;
  onEditTask?: (task: TaskWorkbarItem) => void;
}

const TaskHistoryDrawer = ({
  open,
  historyFilter,
  sortedHistoryTasks,
  processedHistoryTasks,
  visibleHistoryTasks,
  historyLoading = false,
  isLoading = false,
  historyError = null,
  actionLoadingTaskId = null,
  onClose,
  onRefresh,
  onSetHistoryFilter,
  onCompleteTask,
  onDeleteTask,
  onEditTask,
}: TaskHistoryDrawerProps) => {
  if (!open) {
    return null;
  }

  const groupedVisibleTasks = visibleHistoryTasks.reduce<Array<{ planId: string; tasks: TaskWorkbarItem[] }>>(
    (acc, task) => {
      const planId = (task.taskPlanId || task.id || 'ungrouped').trim();
      const last = acc[acc.length - 1];
      if (last && last.planId === planId) {
        last.tasks.push(task);
        return acc;
      }
      acc.push({ planId, tasks: [task] });
      return acc;
    },
    [],
  );

  return (
    <div className="fixed inset-0 z-50">
      <button
        type="button"
        aria-label="关闭历史任务抽屉"
        className="absolute inset-0 bg-black/35"
        onClick={onClose}
      />
      <div className="absolute right-0 top-0 h-full w-full max-w-md border-l border-border bg-card shadow-xl">
        <div className="flex h-full flex-col">
          <div className="flex items-center justify-between border-b border-border px-4 py-3">
            <div>
              <div className="text-sm font-semibold text-foreground">历史任务</div>
              <div className="text-xs text-muted-foreground">
                {`全部 ${sortedHistoryTasks.length} · 已处理 ${processedHistoryTasks.length}`}
              </div>
            </div>
            <div className="flex items-center gap-2">
              {onRefresh ? (
                <button
                  type="button"
                  className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent"
                  onClick={onRefresh}
                  disabled={isLoading || historyLoading}
                >
                  {(isLoading || historyLoading) ? '刷新中...' : '刷新'}
                </button>
              ) : null}
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent"
                onClick={onClose}
              >
                关闭
              </button>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto px-3 py-3">
            <div className="mb-3 inline-flex rounded-md border border-border bg-background p-0.5">
              <button
                type="button"
                className={`rounded px-2.5 py-1 text-xs transition-colors ${
                  historyFilter === 'all'
                    ? 'bg-accent text-foreground'
                    : 'text-muted-foreground hover:bg-accent hover:text-foreground'
                }`}
                onClick={() => onSetHistoryFilter('all')}
              >
                全部
              </button>
              <button
                type="button"
                className={`rounded px-2.5 py-1 text-xs transition-colors ${
                  historyFilter === 'processed'
                    ? 'bg-accent text-foreground'
                    : 'text-muted-foreground hover:bg-accent hover:text-foreground'
                }`}
                onClick={() => onSetHistoryFilter('processed')}
              >
                已处理
              </button>
            </div>

            {historyError ? (
              <div className="mb-2 rounded-md border border-rose-200 bg-rose-50 px-2 py-1 text-xs text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-200">
                {historyError}
              </div>
            ) : null}

            {historyLoading || (isLoading && visibleHistoryTasks.length === 0) ? (
              <div className="text-xs text-muted-foreground">历史任务加载中...</div>
            ) : null}

            {!historyLoading && visibleHistoryTasks.length === 0 ? (
              <div className="text-xs text-muted-foreground">
                {historyFilter === 'processed' ? '暂无已处理待办。' : '暂无历史任务。'}
              </div>
            ) : null}

            {visibleHistoryTasks.length > 0 ? (
              <div className="space-y-3">
                {groupedVisibleTasks.map((group) => (
                  <div key={group.planId} className="space-y-2">
                    <div className="sticky top-0 z-10 rounded-md border border-border bg-background/95 px-2 py-1 text-[11px] font-medium text-foreground backdrop-blur">
                      {`计划 ${group.planId} · ${group.tasks.length} 个任务`}
                    </div>
                    {group.tasks.map((task) => (
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
                ))}
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
};

export default TaskHistoryDrawer;
