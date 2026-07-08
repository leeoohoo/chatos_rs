// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import { cn } from '../../lib/utils';

interface MessageAvatarProps {
  isUser: boolean;
  isAssistant: boolean;
  isSystem: boolean;
  isTool: boolean;
  assistantDisplayName?: string | null;
}

export const MessageAvatar: FC<MessageAvatarProps> = ({
  isUser,
  isAssistant,
  isSystem,
  isTool,
  assistantDisplayName,
}) => {
  const assistantInitial = typeof assistantDisplayName === 'string'
    ? assistantDisplayName.trim().slice(0, 1).toUpperCase()
    : '';
  return (
    <div className="flex-shrink-0">
      <div className={cn(
        'w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium',
        isUser && 'bg-primary text-primary-foreground',
        isAssistant && 'bg-muted text-foreground',
        isSystem && 'bg-muted text-muted-foreground',
        isTool && 'bg-blue-500 text-white'
      )}>
        {isUser ? 'U' : isTool ? 'T' : isAssistant ? (assistantInitial || 'A') : 'S'}
      </div>
    </div>
  );
};
