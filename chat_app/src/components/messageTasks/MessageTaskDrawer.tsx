import type { FC } from 'react';
import { RefreshCw, X } from 'lucide-react';
import type { Message } from '../../types';
import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';
import { MessageTaskCard } from './MessageTaskCard';
import { MessageTaskDetailModal } from './MessageTaskDetailModal';
import { MessageTaskRunDetailModal } from './MessageTaskRunDetailModal';
import { formatDateTime } from './utils';
import { useMessageTasks } from './useMessageTasks';

interface MessageTaskDrawerProps {
  open: boolean;
  message: Message;
  onClose: () => void;
}

export const MessageTaskDrawer: FC<MessageTaskDrawerProps> = ({
  open,
  message,
  onClose,
}) => {
  const { t } = useI18n();
  const {
    tasks,
    sourceUserMessageId,
    loading,
    error,
    detailTask,
    runDetail,
    loadingDetailId,
    loadingRunId,
    reloadTasks,
    openDetail,
    openRun,
    closeDetail,
    closeRun,
  } = useMessageTasks({
    open,
    messageId: message.id,
  });

  const role = message.role === 'user'
    ? t('message.role.user')
    : message.role === 'assistant'
      ? t('message.role.assistant')
      : message.role;
  const messageSummary = `${role} · ${formatDateTime(message.createdAt.toISOString())}`;

  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50">
      <button
        type="button"
        className="absolute inset-0 bg-black/35"
        aria-label="关闭任务抽屉"
        onClick={onClose}
      />
      <aside className="absolute right-0 top-0 h-full w-full max-w-2xl border-l border-border bg-card shadow-xl">
        <div className="flex h-full flex-col">
          <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
            <div className="min-w-0">
              <h2 className="text-sm font-semibold text-foreground">任务</h2>
              <p className="mt-0.5 truncate text-xs text-muted-foreground">{messageSummary}</p>
              <p className="mt-0.5 truncate text-xs text-muted-foreground">
                源消息：{sourceUserMessageId || message.id}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
                disabled={loading}
                onClick={() => void reloadTasks()}
                aria-label="刷新任务"
              >
                <RefreshCw className={cn('h-4 w-4', loading && 'animate-spin')} />
              </button>
              <button
                type="button"
                className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
                onClick={onClose}
                aria-label="关闭"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto px-4 py-4">
            {error ? (
              <div className="mb-3 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
                {error}
              </div>
            ) : null}

            {loading ? (
              <div className="rounded-md border border-border bg-background px-3 py-6 text-center text-sm text-muted-foreground">
                正在读取任务...
              </div>
            ) : tasks.length ? (
              <div className="space-y-3">
                {tasks.map((task) => (
                  <MessageTaskCard
                    key={task.id}
                    task={task}
                    loadingDetail={loadingDetailId === task.id}
                    loadingRun={loadingRunId === task.last_run_id}
                    onOpenDetail={openDetail}
                    onOpenRun={openRun}
                  />
                ))}
              </div>
            ) : (
              <div className="rounded-md border border-dashed border-border bg-background px-3 py-8 text-center text-sm text-muted-foreground">
                这条消息暂无关联任务
              </div>
            )}
          </div>
        </div>
      </aside>

      <MessageTaskDetailModal task={detailTask} onClose={closeDetail} />
      <MessageTaskRunDetailModal detail={runDetail} onClose={closeRun} />
    </div>
  );
};
