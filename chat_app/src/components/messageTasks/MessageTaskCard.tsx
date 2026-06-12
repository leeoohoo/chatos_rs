import type { FC } from 'react';
import { Activity, FileText } from 'lucide-react';
import type { MessageTaskRunnerTask } from '../../lib/api/client/types';
import { readString } from './utils';
import { StatusBadge } from './parts';

interface MessageTaskCardProps {
  task: MessageTaskRunnerTask;
  loadingDetail: boolean;
  loadingRun: boolean;
  onOpenDetail: (task: MessageTaskRunnerTask) => void;
  onOpenRun: (task: MessageTaskRunnerTask) => void;
}

export const MessageTaskCard: FC<MessageTaskCardProps> = ({
  task,
  loadingDetail,
  loadingRun,
  onOpenDetail,
  onOpenRun,
}) => {
  const description = readString(task.description) || '暂无描述';

  return (
    <article className="rounded-md border border-border bg-background p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="break-words text-sm font-semibold text-foreground">
              {task.title || task.id}
            </h3>
            <StatusBadge status={task.status} />
          </div>
          <p className="mt-2 whitespace-pre-wrap break-words text-sm text-muted-foreground">
            {description}
          </p>
        </div>
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          className="inline-flex items-center gap-1 rounded-md border border-border bg-card px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-60"
          disabled={loadingDetail}
          onClick={() => onOpenDetail(task)}
        >
          <FileText className="h-3.5 w-3.5" />
          详情
        </button>
        <button
          type="button"
          className="inline-flex items-center gap-1 rounded-md border border-border bg-card px-2 py-1 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60"
          disabled={loadingRun || !task.last_run_id}
          onClick={() => onOpenRun(task)}
        >
          <Activity className="h-3.5 w-3.5" />
          运行详情
        </button>
      </div>
    </article>
  );
};
