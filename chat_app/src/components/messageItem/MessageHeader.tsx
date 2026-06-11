import type { FC } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import { formatTime } from '../../lib/utils';
import type { Message } from '../../types';

interface MessageHeaderProps {
  message: Message;
  isUser: boolean;
  isAssistant: boolean;
  isTool: boolean;
  assistantDisplayName?: string | null;
}

export const MessageHeader: FC<MessageHeaderProps> = ({
  message,
  isUser,
  isAssistant,
  isTool,
  assistantDisplayName,
}) => {
  const { t } = useI18n();
  const taskRunnerMode = isUser
    ? String(message.metadata?.task_runner_async?.mode || '').trim().toLowerCase()
    : '';
  const taskRunnerStatus = isUser && taskRunnerMode === 'contact_async'
    ? String(message.metadata?.task_runner_async?.overall_status || '').trim().toLowerCase()
    : '';
  const taskRunnerStatusConfig = taskRunnerStatus === 'processing'
    ? { label: '正在处理', className: 'text-amber-700 bg-amber-50 border border-amber-200' }
    : taskRunnerStatus === 'completed'
      ? { label: '已处理', className: 'text-emerald-700 bg-emerald-50 border border-emerald-200' }
      : taskRunnerStatus === 'pending'
        ? { label: '待处理', className: 'text-sky-700 bg-sky-50 border border-sky-200' }
        : null;

  return (
    <div className="flex items-center gap-2 mb-1">
      <span className="text-sm font-medium">
        {isUser ? t('message.role.user') : isTool ? t('message.role.toolResult') : isAssistant ? (assistantDisplayName || t('message.role.assistant')) : t('message.role.system')}
      </span>
      <span className="text-xs text-muted-foreground">
        {formatTime(message.createdAt)}
      </span>
      {taskRunnerStatusConfig && (
        <span className={`text-[11px] px-1.5 py-0.5 rounded ${taskRunnerStatusConfig.className}`}>
          {taskRunnerStatusConfig.label}
        </span>
      )}
      {message.metadata?.model && (
        <span className="text-xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
          {message.metadata.model}
        </span>
      )}
    </div>
  );
};
