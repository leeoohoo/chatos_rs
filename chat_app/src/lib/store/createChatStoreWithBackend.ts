import {create} from 'zustand';
import {immer} from 'zustand/middleware/immer';
import {persist} from 'zustand/middleware';
import {apiClient} from '../api/client';
import type ApiClient from '../api/client';
import {createSendMessageHandler} from './actions/sendMessage';
import { createApplicationActions } from './actions/applications';
import { createAiModelActions } from './actions/aiModels';
import { createMcpActions } from './actions/mcp';
import { createChatConfigActions } from './actions/chatConfig';
import { createSessionActions } from './actions/sessions';
import { createProjectActions } from './actions/projects';
import { createTerminalActions } from './actions/terminals';
import { createMessageActions } from './actions/messages';
import { createStreamingActions } from './actions/streaming';
import { createAgentActions } from './actions/agents';
import { createSystemContextActions } from './actions/systemContexts';
import { createUiActions } from './actions/ui';
import type { ChatActions, ChatState, ChatStoreConfig } from './types';

export type { ChatActions, ChatState, ChatStoreConfig } from './types';

/**
 * 创建聊天store的工厂函数（使用后端API版本）
 * @param customApiClient 自定义的API客户端实例，如果不提供则使用默认的apiClient
 * @param config 自定义配置，包含userId和projectId
 * @returns 聊天store hook
 */
export function createChatStoreWithBackend(customApiClient?: ApiClient, config?: ChatStoreConfig) {
    const client = customApiClient || apiClient;
    const customUserId = config?.userId;
    const customProjectId = config?.projectId;
    
    // 使用传入的参数或默认值
    const userId = customUserId || 'default-user';
    
    // 获取userId的统一函数
    const getUserIdParam = () => userId;
    
    return create<ChatState & ChatActions>()(
        immer(
            persist(
                    (set, get) => {
                    const getSessionParams = () => ({
                        userId,
                        projectId: '',
                    });

                    return {
                    // 初始状态
                    sessions: [],
                    currentSessionId: null,
                    currentSession: null,
                    projects: [],
                    currentProjectId: null,
                    currentProject: null,
                    activePanel: 'chat',
                    terminals: [],
                    currentTerminalId: null,
                    currentTerminal: null,
                    messages: [],
                    isLoading: false,
                    isStreaming: false,
                    streamingMessageId: null,
                    hasMoreMessages: true,
                    sessionChatState: {},
                    sidebarOpen: true,
                    theme: 'light',
                    chatConfig: {
                        model: 'gpt-4',
                        temperature: 0.7,
                        systemPrompt: '',
                        enableMcp: true,
                        reasoningEnabled: false,
                    },
                    mcpConfigs: [],
                    aiModelConfigs: [],
                    selectedModelId: null,
                    agents: [],
                    selectedAgentId: null,
                    systemContexts: [],
                    activeSystemContext: null,
                    applications: [],
                    selectedApplicationId: null,
                    error: null,

                    // 会话/项目/消息/流式/UI 操作（拆分到独立模块）
                    ...createSessionActions({ set, get, client, getSessionParams, customUserId, customProjectId }),
                    ...createProjectActions({ set, get, client, getUserIdParam }),
                    ...createTerminalActions({ set, get, client, getUserIdParam }),
                    ...createMessageActions({ set, get, client }),
                    sendMessage: createSendMessageHandler({ set, get, client, getUserIdParam }),
                    ...createStreamingActions({ set, get, client }),
                    ...createUiActions({ set }),

                    // 配置操作（拆分到独立模块）
                    ...createChatConfigActions({ set, get }),

                    // MCP 管理（拆分到独立模块）
                    ...createMcpActions({ set, get, client, getUserIdParam }),

                    // 应用管理（拆分到独立模块）
                    ...createApplicationActions({ set, get, client, getUserIdParam }),

                    // AI模型管理（拆分到独立模块）
                    ...createAiModelActions({ set, get, client, getUserIdParam }),

                    // 智能体/系统上下文（拆分到独立模块）
                    ...createAgentActions({ set, client, getUserIdParam }),
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
                    name: 'chat-store-with-backend',
                    partialize: (state) => ({
                        theme: state.theme,
                        sidebarOpen: state.sidebarOpen,
                        chatConfig: state.chatConfig,
                        selectedModelId: state.selectedModelId,
                    }),
                }
            )
    ));
}

// 导出 ChatStore 类型别名，供外部命名使用
export type ChatStore = ReturnType<typeof createChatStoreWithBackend>;
