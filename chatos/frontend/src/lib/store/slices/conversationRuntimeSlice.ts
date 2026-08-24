// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  Message,
  SendMessageRuntimeOptions,
} from '../../../types';

export interface SessionChatState {
  isLoading: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  streamingPhase?: 'thinking' | 'reviewing' | null;
  streamingMessageId: string | null;
  activeTurnId: string | null;
  streamingPreviewText: string;
  streamingTransport?: 'realtime' | null;
  runtimeContextRefreshNonce?: number;
}

export interface SessionMessagePaginationState {
  nextBefore: string | null;
  loaded: boolean;
}

export interface ConversationRuntimeSliceState {
  messages: Message[];
  isLoading: boolean;
  isStreaming: boolean;
  streamingMessageId: string | null;
  hasMoreMessages: boolean;
  sessionChatState: Record<string, SessionChatState>;
  sessionMessagePaginationState: Record<string, SessionMessagePaginationState>;
}

export const conversationRuntimeInitialState: ConversationRuntimeSliceState = {
  messages: [],
  isLoading: false,
  isStreaming: false,
  streamingMessageId: null,
  hasMoreMessages: true,
  sessionChatState: {},
  sessionMessagePaginationState: {},
};

export interface ConversationRuntimeSliceActions {
  loadMessages: (sessionId: string) => Promise<void>;
  syncSessionMessagesInBackground: (sessionId: string) => Promise<void>;
  loadMoreMessages: (sessionId: string) => Promise<void>;
  upsertSessionMessage: (message: Message) => void;
  sendMessage: (
    content: string,
    attachments?: File[],
    runtimeOptions?: SendMessageRuntimeOptions,
  ) => Promise<void>;
  stopMessage: () => Promise<void>;
  updateMessage: (messageId: string, updates: Partial<Message>) => Promise<void>;
  deleteMessage: (messageId: string) => Promise<void>;
}
