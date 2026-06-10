import { useI18n } from '../../i18n/I18nProvider';
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
  const { t } = useI18n();

  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50">
      <button
        type="button"
        aria-label={t('taskWorkbar.history.closeLabel')}
        className="absolute inset-0 bg-black/35"
        onClick={onClose}
      />
      <div className="absolute right-0 top-0 h-full w-full max-w-md border-l border-border bg-card shadow-xl">
        <div className="flex h-full flex-col">
          <div className="flex items-center justify-between border-b border-border px-4 py-3">
            <div>
              <div className="text-sm font-semibold text-foreground">{t('taskWorkbar.history')}</div>
              <div className="text-xs text-muted-foreground">
                {t('taskWorkbar.history.total', {
                  total: sortedHistoryTasks.length,
                  processed: processedHistoryTasks.length,
                })}
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
                  {(isLoading || historyLoading) ? t('taskWorkbar.refreshing') : t('taskWorkbar.refresh')}
                </button>
              ) : null}
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent"
                onClick={onClose}
              >
                {t('common.close')}
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
                {t('taskWorkbar.history.all')}
              </button>
              <button
                type="button"
                className={`rounded px-2.5 py-1 text-xs transition-colors ${
                  historyFilter === 'unfinished'
                    ? 'bg-accent text-foreground'
                    : 'text-muted-foreground hover:bg-accent hover:text-foreground'
                }`}
                onClick={() => onSetHistoryFilter('unfinished')}
              >
                {t('taskWorkbar.history.unfinished')}
              </button>
              <button
                type="button"
                className={`rounded px-2.5 py-1 text-xs transition-colors ${
                  historyFilter === 'blocked'
                    ? 'bg-accent text-foreground'
                    : 'text-muted-foreground hover:bg-accent hover:text-foreground'
                }`}
                onClick={() => onSetHistoryFilter('blocked')}
              >
                {t('taskWorkbar.history.blocked')}
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
                {t('taskWorkbar.processed')}
              </button>
            </div>

            {historyError ? (
              <div className="mb-2 rounded-md border border-rose-200 bg-rose-50 px-2 py-1 text-xs text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-200">
                {historyError}
              </div>
            ) : null}

            {historyLoading || (isLoading && visibleHistoryTasks.length === 0) ? (
              <div className="text-xs text-muted-foreground">{t('taskWorkbar.loading')}</div>
            ) : null}

            {!historyLoading && visibleHistoryTasks.length === 0 ? (
              <div className="text-xs text-muted-foreground">
                {historyFilter === 'processed'
                  ? t('taskWorkbar.history.emptyProcessed')
                  : historyFilter === 'unfinished'
                    ? t('taskWorkbar.history.emptyUnfinished')
                    : historyFilter === 'blocked'
                      ? t('taskWorkbar.history.emptyBlocked')
                      : t('taskWorkbar.history.emptyAll')}
              </div>
            ) : null}

            {visibleHistoryTasks.length > 0 ? (
              <div className="space-y-2">
                {visibleHistoryTasks.map((task) => (
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
  );
};

export default TaskHistoryDrawer;
