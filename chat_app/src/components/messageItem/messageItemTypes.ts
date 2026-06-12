import type { ReactNode } from 'react';
import type { Attachment, Message, ToolCall } from '../../types';

export interface MessageItemProps {
  message: Message;
  isLast?: boolean;
  isStreaming?: boolean;
  assistantContactName?: string | null;
  onEdit?: (messageId: string, content: string) => void;
  onDelete?: (messageId: string) => void;
  toolResultById?: Map<string, Message>;
  assistantToolCallsById?: Map<string, ToolCall>;
  toolResultKey?: string;
  toolCallLookupKey?: string;
  onOpenTasks?: (message: Message) => void;
  customRenderer?: {
    renderMessage?: (message: Message) => ReactNode;
    renderAttachment?: (attachment: Attachment) => ReactNode;
  };
}
