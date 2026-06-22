import React from 'react';
import { ListTodo } from 'lucide-react';

import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';
import type { Message } from '../../types';
import type { UserMessageTurn } from './types';

interface ConversationUserMessageItemProps {
  item: UserMessageTurn;
  active: boolean;
  onSelect: () => void;
  onOpenTasks: (message: Message) => void;
}

const clipText = (value: string, limit = 110): string => {
  const normalized = value.replace(/\s+/g, ' ').trim();
  return normalized.length > limit ? `${normalized.slice(0, limit)}...` : normalized;
};

const formatTime = (date: Date): string => {
  if (Number.isNaN(date.getTime())) {
    return '';
  }
  return date.toLocaleString();
};

export const ConversationUserMessageItem: React.FC<ConversationUserMessageItemProps> = ({
  item,
  active,
  onSelect,
  onOpenTasks,
}) => {
  const { t } = useI18n();
  const { userMessage, taskState } = item;

  return (
    <div
      role="button"
      tabIndex={0}
      className={cn(
        'border-l-2 px-4 py-2.5 text-left transition-colors',
        active
          ? 'border-l-primary bg-primary/10'
          : taskState.running
            ? 'border-l-amber-400 bg-amber-50/50 hover:bg-amber-50'
            : 'border-l-transparent hover:bg-accent/60',
      )}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect();
        }
      }}
    >
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
            {taskState.running ? (
              <span className="inline-flex items-center gap-1 text-amber-700">
                <span className="h-1.5 w-1.5 rounded-full bg-amber-500" />
                {t('projectUserMessages.running')}
              </span>
            ) : null}
            <span className="truncate">
              {item.hasProcess
                ? t('projectUserMessages.processCount', { count: item.processMessageCount })
                : t('projectUserMessages.noProcess')}
            </span>
            <span className="shrink-0 text-border">/</span>
            <span className="shrink-0">
              {formatTime(userMessage.createdAt)}
            </span>
          </div>
          <div className="mt-1 line-clamp-2 text-sm font-semibold leading-5 text-foreground">
            {clipText(userMessage.content) || userMessage.id}
          </div>
        </div>

        {taskState.hasTask ? (
          <button
            type="button"
            className="mt-4 inline-flex shrink-0 items-center gap-1 rounded border border-border bg-background px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground"
            onClick={(event) => {
              event.stopPropagation();
              onOpenTasks(userMessage);
            }}
          >
            <ListTodo className="h-3.5 w-3.5" />
            {t('projectUserMessages.openTasks')}
          </button>
        ) : null}
      </div>
    </div>
  );
};
