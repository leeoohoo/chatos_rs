import { useI18n } from '../../i18n/I18nProvider';
import {
  priorityStyles,
  statusStyles,
} from './helpers';
import type { TaskWorkbarItem } from './types';

interface TaskCardProps {
  task: TaskWorkbarItem;
  compact?: boolean;
  onCompleteTask?: (task: TaskWorkbarItem) => void;
  onDeleteTask?: (task: TaskWorkbarItem) => void;
  onEditTask?: (task: TaskWorkbarItem) => void;
  isMutating?: boolean;
}

const TaskCard = ({
  task,
  compact = false,
  onCompleteTask,
  onDeleteTask,
  onEditTask,
  isMutating = false,
}: TaskCardProps) => {
  const { locale, t } = useI18n();
  const cardClass = compact
    ? 'min-w-[160px] max-w-[190px] min-w-0 overflow-hidden rounded-md border border-border bg-background p-2'
    : 'min-w-0 overflow-hidden rounded-lg border border-border bg-background p-2.5';

  const titleClass = compact
    ? 'min-w-0 line-clamp-2 break-words text-xs font-medium text-foreground'
    : 'min-w-0 line-clamp-2 break-words text-sm font-medium text-foreground';

  const detailsClass = compact
    ? 'mb-1 line-clamp-1 break-all text-[11px] text-muted-foreground'
    : 'mb-1 line-clamp-2 break-all text-xs text-muted-foreground';
  const secondaryTextClass = compact
    ? 'mb-1 line-clamp-2 break-all text-[11px] text-muted-foreground'
    : 'mb-1 line-clamp-3 break-all text-xs text-muted-foreground';

  const metaClass = compact ? 'text-[10px] text-muted-foreground' : 'text-[11px] text-muted-foreground';
  const actionClass = compact
    ? 'rounded border border-border bg-background px-1.5 py-0.5 text-[10px] text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50'
    : 'rounded border border-border bg-background px-2 py-0.5 text-[11px] text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50';
  const outcomeSummary = task.outcomeSummary.trim();
  const blockerReason = task.blockerReason.trim();

  return (
    <div className={cardClass}>
      <div className="mb-1 flex min-w-0 items-start justify-between gap-2">
        <div className={titleClass}>{task.title}</div>
        <span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${statusStyles[task.status]}`}>
          {t(`taskWorkbar.status.${task.status}`)}
        </span>
      </div>

      {task.details ? <div className={detailsClass}>{task.details}</div> : null}
      {outcomeSummary ? (
        <div className={secondaryTextClass} title={outcomeSummary}>
          {t('taskWorkbar.card.outcome', { value: outcomeSummary })}
        </div>
      ) : null}
      {task.status === 'blocked' && blockerReason ? (
        <div className={secondaryTextClass} title={blockerReason}>
          {t('taskWorkbar.card.blocker', { value: blockerReason })}
        </div>
      ) : null}

      <div className={metaClass}>
        <div>
          <span className={priorityStyles[task.priority]}>
            {t('taskWorkbar.card.priority', { value: t(`taskWorkbar.priority.${task.priority}`) })}
          </span>
        </div>
        <div className="truncate" title={task.conversationTurnId}>
          {t('taskWorkbar.card.turn', { value: task.conversationTurnId })}
        </div>
      </div>

      {(onCompleteTask || onEditTask || onDeleteTask) ? (
        <div className={compact ? 'mt-1 flex items-center gap-1' : 'mt-2 flex items-center gap-1'}>
          {onCompleteTask && task.status !== 'done' ? (
            <button type="button" className={actionClass} onClick={() => onCompleteTask(task)} disabled={isMutating}>
              {t('taskWorkbar.card.complete')}
            </button>
          ) : null}
          {onEditTask ? (
            <button type="button" className={actionClass} onClick={() => onEditTask(task)} disabled={isMutating}>
              {t('taskWorkbar.card.edit')}
            </button>
          ) : null}
          {onDeleteTask ? (
            <button type="button" className={actionClass} onClick={() => onDeleteTask(task)} disabled={isMutating}>
              {t('taskWorkbar.card.delete')}
            </button>
          ) : null}
          {isMutating ? (
            <span className={compact ? 'text-[10px] text-muted-foreground' : 'text-[11px] text-muted-foreground'}>
              {t('taskWorkbar.card.processing')}
            </span>
          ) : null}
        </div>
      ) : null}

      {task.dueAt ? (
        <div className={compact ? 'mt-1 truncate text-[10px] text-muted-foreground' : 'mt-1 truncate text-[11px] text-muted-foreground'} title={task.dueAt}>
          {t('taskWorkbar.card.due', { value: task.dueAt })}
        </div>
      ) : null}
      {task.status === 'blocked' && task.blockerNeeds.length > 0 ? (
        <div
          className={compact ? 'mt-1 line-clamp-2 text-[10px] text-muted-foreground' : 'mt-1 line-clamp-3 text-[11px] text-muted-foreground'}
          title={task.blockerNeeds.join(locale === 'zh-CN' ? '；' : '; ')}
        >
          {t('taskWorkbar.card.needs', {
            value: task.blockerNeeds.join(locale === 'zh-CN' ? '；' : '; '),
          })}
        </div>
      ) : null}
    </div>
  );
};

export default TaskCard;
