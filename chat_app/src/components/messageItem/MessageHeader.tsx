import type { FC } from 'react';
import { formatTime } from '../../lib/utils';
import type { Message } from '../../types';

interface MessageHeaderProps {
  message: Message;
  isUser: boolean;
  isAssistant: boolean;
  isTool: boolean;
}

export const MessageHeader: FC<MessageHeaderProps> = ({
  message,
  isUser,
  isAssistant,
  isTool,
}) => {
  const taskRunnerStatus = isUser
    ? String(message.metadata?.task_runner_async?.overall_status || '').trim().toLowerCase()
    : '';
  const taskRunnerStatusLabel = taskRunnerStatus === 'processing'
    ? '正在处理'
    : taskRunnerStatus === 'completed'
      ? '已处理'
      : taskRunnerStatus === 'pending'
        ? '待处理'
        : '';

  return (
    <div className="flex items-center gap-2 mb-1">
      <span className="text-sm font-medium">
        {isUser ? 'You' : isTool ? 'Tool Result' : isAssistant ? 'Assistant' : 'System'}
      </span>
      <span className="text-xs text-muted-foreground">
        {formatTime(message.createdAt)}
      </span>
      {taskRunnerStatusLabel && (
        <span className="text-xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
          {taskRunnerStatusLabel}
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
