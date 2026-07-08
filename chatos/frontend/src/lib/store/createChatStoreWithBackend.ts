// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { createWithEqualityFn } from 'zustand/traditional';
import {immer} from 'zustand/middleware/immer';
import {persist} from 'zustand/middleware';
import type ApiClient from '../api/client';
import {createSendMessageHandler} from './actions/sendMessage';
import { createApplicationActions } from './actions/applications';
import { createAiModelActions } from './actions/aiModels';
import { createMcpActions } from './actions/mcp';
import { createChatConfigActions } from './actions/chatConfig';
import { createSessionActions } from './actions/sessions';
import { createContactActions } from './actions/contacts';
import { createProjectActions } from './actions/projects';
import { createTerminalActions } from './actions/terminals';
import { createRemoteConnectionActions } from './actions/remoteConnections';
import { createMessageActions } from './actions/messages';
import { createAgentActions } from './actions/agents';
import { createSystemContextActions } from './actions/systemContexts';
import { createUiActions } from './actions/ui';
import { configurationInitialState } from './slices/configurationSlice';
import { conversationRuntimeInitialState } from './slices/conversationRuntimeSlice';
import { remoteExecutionInitialState } from './slices/remoteExecutionSlice';
import { sessionInitialState } from './slices/sessionSlice';
import { uiInitialState } from './slices/uiSlice';
import { workspaceInitialState } from './slices/workspaceSlice';
import {
  primeScopedChatStoreStateFromLegacy,
  resolveChatStorePersistKey,
} from './persistence';
import type {
  ChatActions,
  ChatState,
  ChatStoreConfig,
} from './types';

export type { ChatActions, ChatState, ChatStoreConfig } from './types';

/**
 * 创建聊天store的工厂函数（使用后端API版本）
 * @param customApiClient 自定义的API客户端实例
 * @param config 自定义配置，包含userId和projectId
 * @returns 聊天store hook
 */
export function createChatStoreWithBackend(customApiClient: ApiClient, config?: ChatStoreConfig) {
    const client = customApiClient;
    const customUserId = config?.userId;
    const customProjectId = config?.projectId;
    
    // 用户 ID 由登录态注入；缺失时不再回退到硬编码默认值
    const userId = customUserId || '';
    const persistKey = resolveChatStorePersistKey(userId);
    primeScopedChatStoreStateFromLegacy(userId);
    
    // 获取userId的统一函数
    const getUserIdParam = () => userId;
    
    return createWithEqualityFn<ChatState & ChatActions>()(
        immer(
            persist(
                    (set, get) => {
                    const getSessionParams = () => ({
                        userId,
                        projectId: customProjectId || get().currentProjectId || '',
                    });

                    return {
                    ...sessionInitialState,
                    ...workspaceInitialState,
                    ...remoteExecutionInitialState,
                    ...conversationRuntimeInitialState,
                    ...uiInitialState,
                    ...configurationInitialState,

                    // 会话/项目/消息/流式/UI 操作（拆分到独立模块）
                    ...createContactActions({ set, get, client, getUserIdParam }),
                    ...createSessionActions({ set, get, client, getSessionParams, customUserId, customProjectId }),
                    ...createProjectActions({ set, get, client, getUserIdParam }),
                    ...createTerminalActions({ set, get, client, getUserIdParam }),
                    ...createRemoteConnectionActions({ set, get, client, getUserIdParam }),
                    ...createMessageActions({ set, get, client }),
                    sendMessage: createSendMessageHandler({ set, get, client, getUserIdParam }),
                    ...createUiActions({ set }),

                    // 配置操作（拆分到独立模块）
                    ...createChatConfigActions({ set, get }),

                    // MCP 管理（拆分到独立模块）
                    ...createMcpActions({ set, get, client, getUserIdParam }),

                    // 应用管理（拆分到独立模块）
                    ...createApplicationActions({ set, get, client, getUserIdParam }),

                    // AI模型管理（拆分到独立模块）
                    ...createAiModelActions({ set, get, client }),

                    // 智能体/系统上下文（拆分到独立模块）
                    ...createAgentActions({ set, get, client, getUserIdParam }),
                    ...createSystemContextActions({ set, client, getUserIdParam }),

                    // 错误处理
                    setError: (error: string | null) => {
                        set((state) => {
                            state.error = error;
                        });
                    },

                    clearError: () => {
                        set((state) => {
                            state.error = null;
                        });
                    },
                };
                },
                {
                    name: persistKey,
                    version: 2,
                    migrate: (persistedState: unknown, version: number) => {
                        if (!persistedState || typeof persistedState !== 'object') {
                            return persistedState;
                        }
                        const nextState = {...(persistedState as Record<string, unknown>)};
                        if (version < 2) {
                            delete nextState.sessionChatState;
                        }
                        delete nextState.sessionStreamingMessageDrafts;
                        delete nextState.sessionTurnProcessCache;
                        return nextState;
                    },
                    partialize: (state) => ({
                        theme: state.theme,
                        sidebarOpen: state.sidebarOpen,
                        chatConfig: state.chatConfig,
                        selectedModelId: state.selectedModelId,
                        selectedAgentId: state.selectedAgentId,
                        sessionAiSelectionBySession: state.sessionAiSelectionBySession,
                    }),
                }
            )
    ));
}

// 导出 ChatStore 类型别名，供外部命名使用
export type ChatStore = ReturnType<typeof createChatStoreWithBackend>;
