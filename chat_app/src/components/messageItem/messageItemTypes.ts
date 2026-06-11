import type { ReactNode } from 'react';
import type { Attachment, Message, ToolCall } from '../../types';
import type { DerivedProcessStats } from './types';

export interface MessageItemProps {
  message: Message;
  isLast?: boolean;
  isStreaming?: boolean;
  assistantContactName?: string | null;
  onEdit?: (messageId: string, content: string) => void;
  onDelete?: (messageId: string) => void;
  onToggleTurnProcess?: (userMessageId: string) => void;
  hideHistoryProcessSummary?: boolean;
  derivedProcessStatsByUserId?: Map<string, DerivedProcessStats>;
  toolResultById?: Map<string, Message>;
  assistantToolCallsById?: Map<string, ToolCall>;
  toolResultKey?: string;
  toolCallLookupKey?: string;
  processSignal?: string;
  customRenderer?: {
    renderMessage?: (message: Message) => ReactNode;
    renderAttachment?: (attachment: Attachment) => ReactNode;
  };
}
