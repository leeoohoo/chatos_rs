import type { FC } from 'react';
import { ListTodo } from 'lucide-react';
import { useI18n } from '../../i18n/I18nProvider';

interface MessageActionsProps {
  isUser: boolean;
  canEdit: boolean;
  canDelete: boolean;
  onOpenTasks?: () => void;
  onCopy: () => void;
  onStartEdit: () => void;
  onDelete: () => void;
}

export const MessageActions: FC<MessageActionsProps> = ({
  isUser,
  canEdit,
  canDelete,
  onOpenTasks,
  onCopy,
  onStartEdit,
  onDelete,
}) => {
  const { t } = useI18n();

  return (
    <div className="absolute top-2 right-2 flex gap-1 bg-background border rounded-md shadow-sm opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto transition-opacity">
      {onOpenTasks && (
        <button
          onClick={onOpenTasks}
          className="inline-flex items-center gap-1 rounded px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          title={t('messageTasks.action')}
        >
          <ListTodo className="h-4 w-4" />
          <span>{t('messageTasks.action')}</span>
        </button>
      )}

      <button
        onClick={onCopy}
        className="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-foreground transition-colors"
        title="Copy message"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
        </svg>
      </button>

      {isUser && canEdit && (
        <button
          onClick={onStartEdit}
          className="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-foreground transition-colors"
          title="Edit message"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
          </svg>
        </button>
      )}

      {canDelete && (
        <button
          onClick={onDelete}
          className="p-1.5 hover:bg-destructive/10 rounded text-muted-foreground hover:text-destructive transition-colors"
          title="Delete message"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>
      )}
    </div>
  );
};
