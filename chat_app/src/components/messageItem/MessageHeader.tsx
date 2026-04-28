import type { FC } from 'react';
import { formatTime } from '../../lib/utils';
import type { Message } from '../../types';

interface MessageHeaderProps {
  message: Message;
  isUser: boolean;
  isTool: boolean;
}

export const MessageHeader: FC<MessageHeaderProps> = ({
  message,
  isUser,
  isTool,
}) => (
  <div className="flex items-center gap-2 mb-1">
    <span className="text-sm font-medium">
      {isUser ? 'You' : isTool ? 'Tool Result' : 'System'}
    </span>
    <span className="text-xs text-muted-foreground">
      {formatTime(message.createdAt)}
    </span>
    {message.metadata?.model && (
      <span className="text-xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
        {message.metadata.model}
      </span>
    )}
  </div>
);
