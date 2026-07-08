// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ConfigurationSliceActions,
  ConfigurationSliceState,
} from './slices/configurationSlice';
import type {
  ConversationRuntimeSliceActions,
  ConversationRuntimeSliceState,
} from './slices/conversationRuntimeSlice';
import type {
  RemoteExecutionSliceActions,
  RemoteExecutionSliceState,
} from './slices/remoteExecutionSlice';
import type {
  SessionSliceActions,
  SessionSliceState,
} from './slices/sessionSlice';
import type {
  UiSliceActions,
  UiSliceState,
} from './slices/uiSlice';
import type {
  WorkspaceSliceActions,
  WorkspaceSliceState,
} from './slices/workspaceSlice';

export type { ContactRecord, SendMessageRuntimeOptions } from '../../types';
export type {
  ConfigurationSliceActions,
  ConfigurationSliceState,
} from './slices/configurationSlice';
export type {
  ConversationRuntimeSliceActions,
  ConversationRuntimeSliceState,
  SessionChatState,
  SessionMessagePaginationState,
  SessionMessagesCacheEntry,
  SessionMessagesSnapshot,
} from './slices/conversationRuntimeSlice';
export type {
  RemoteConnectionCreatePayload,
  RemoteConnectionMutationPayload,
  RemoteExecutionSliceActions,
  RemoteExecutionSliceState,
} from './slices/remoteExecutionSlice';
export type {
  SessionAiSelection,
  SessionCreateOptions,
  SessionCreatePayload,
  SessionSelectOptions,
  SessionSliceActions,
  SessionSliceState,
} from './slices/sessionSlice';
export type {
  UiSliceActions,
  UiSliceState,
} from './slices/uiSlice';
export type {
  ActivePanel,
  WorkspaceSliceActions,
  WorkspaceSliceState,
} from './slices/workspaceSlice';

export interface ChatState
  extends SessionSliceState,
    WorkspaceSliceState,
    RemoteExecutionSliceState,
    ConversationRuntimeSliceState,
    UiSliceState,
    ConfigurationSliceState {}

export interface ChatActions
  extends SessionSliceActions,
    WorkspaceSliceActions,
    RemoteExecutionSliceActions,
    ConversationRuntimeSliceActions,
    UiSliceActions,
    ConfigurationSliceActions {}

export interface ChatStoreConfig {
  userId?: string;
  projectId?: string;
}

export type ChatStoreShape = ChatState & ChatActions;
export type ChatStoreDraft = ChatState & Partial<ChatActions>;
export type ChatStoreSet = (fn: (state: ChatStoreDraft) => void) => void;
export type ChatStoreGet = () => ChatStoreShape;
