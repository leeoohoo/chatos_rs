import type { FC } from 'react';
import { cn } from '../../lib/utils';

interface MessageAvatarProps {
  isUser: boolean;
  isSystem: boolean;
  isTool: boolean;
}

export const MessageAvatar: FC<MessageAvatarProps> = ({
  isUser,
  isSystem,
  isTool,
}) => (
  <div className="flex-shrink-0">
    <div className={cn(
      'w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium',
      isUser && 'bg-primary text-primary-foreground',
      isSystem && 'bg-muted text-muted-foreground',
      isTool && 'bg-blue-500 text-white'
    )}>
      {isUser ? 'U' : isTool ? 'T' : 'S'}
    </div>
  </div>
);
