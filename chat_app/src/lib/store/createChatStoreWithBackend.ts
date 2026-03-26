import { createWithEqualityFn } from 'zustand/traditional';
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
import { createContactActions } from './actions/contacts';
import { createProjectActions } from './actions/projects';
import { createTerminalActions } from './actions/terminals';
import { createRemoteConnectionActions } from './actions/remoteConnections';
import { createMessageActions } from './actions/messages';
import { createStreamingActions } from './actions/streaming';
import { createAgentActions } from './actions/agents';
import { createSystemContextActions } from './actions/systemContexts';
import { createUiActions } from './actions/ui';
import type {
  ChatActions,
  ChatState,
  ChatStoreConfig,
  TaskReviewPanelState,
  UiPromptPanelState,
} from './types';

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
    
    // 用户 ID 由登录态注入；缺失时不再回退到硬编码默认值
    const userId = customUserId || '';
    
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
                    // 初始状态
                    sessions: [],
                    currentSessionId: null,
                    currentSession: null,
                    contacts: [],
                    projects: [],
                    currentProjectId: null,
                    currentProject: null,
                    activePanel: 'chat',
                    terminals: [],
                    currentTerminalId: null,
                    currentTerminal: null,
                    remoteConnections: [],
                    currentRemoteConnectionId: null,
                    currentRemoteConnection: null,
                    messages: [],
                    isLoading: false,
                    isStreaming: false,
                    streamingMessageId: null,
                    hasMoreMessages: true,
                    sessionChatState: {},
                    sessionRuntimeGuidanceState: {},
                    sessionStreamingMessageDrafts: {},
                    sessionTurnProcessState: {},
                    sessionTurnProcessCache: {},
                    taskReviewPanel: null,
                    taskReviewPanelsBySession: {},
                    uiPromptPanel: null,
                    uiPromptPanelsBySession: {},
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
                    sessionAiSelectionBySession: {},
                    systemContexts: [],
                    activeSystemContext: null,
                    applications: [],
                    selectedApplicationId: null,
                    error: null,

                    // 会话/项目/消息/流式/UI 操作（拆分到独立模块）
                    ...createContactActions({ set, get, client, getUserIdParam }),
                    ...createSessionActions({ set, get, client, getSessionParams, customUserId, customProjectId }),
                    ...createProjectActions({ set, get, client, getUserIdParam }),
                    ...createTerminalActions({ set, get, client, getUserIdParam }),
                    ...createRemoteConnectionActions({ set, get, client, getUserIdParam }),
                    ...createMessageActions({ set, get, client }),
                    sendMessage: createSendMessageHandler({ set, get, client, getUserIdParam }),
                    submitRuntimeGuidance: async (
                      content: string,
                      options: { sessionId: string; turnId: string; projectId?: string | null },
                    ) => {
                      const sessionId = String(options?.sessionId || '').trim();
                      const turnId = String(options?.turnId || '').trim();
                      const trimmedContent = String(content || '').trim();
                      const projectId = typeof options?.projectId === 'string'
                        ? options.projectId.trim()
                        : '';
                      if (!sessionId || !turnId || !trimmedContent) {
                        throw new Error('缺少运行时引导参数');
                      }

                      const guidanceAt = new Date().toISOString();
                      const optimisticGuidanceId = `local_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
                      const maxItems = 20;
                      set((state: any) => {
                        if (!state.sessionRuntimeGuidanceState) {
                          state.sessionRuntimeGuidanceState = {};
                        }
                        const prev = state.sessionRuntimeGuidanceState[sessionId] || {
                          pendingCount: 0,
                          appliedCount: 0,
                          lastGuidanceAt: null,
                          lastAppliedAt: null,
                          items: [],
                        };
                        const prevItems = Array.isArray(prev.items) ? prev.items : [];
                        state.sessionRuntimeGuidanceState[sessionId] = {
                          ...prev,
                          pendingCount: Math.max(0, Number(prev.pendingCount || 0) + 1),
                          lastGuidanceAt: guidanceAt,
                          items: [
                            {
                              guidanceId: optimisticGuidanceId,
                              turnId,
                              content: trimmedContent,
                              status: 'queued',
                              createdAt: guidanceAt,
                              appliedAt: null,
                            },
                            ...prevItems,
                          ].slice(0, maxItems),
                        };
                      });

                      try {
                        const response = await client.submitRuntimeGuidance({
                          sessionId,
                          turnId,
                          content: trimmedContent,
                          projectId: projectId || undefined,
                        });
                        set((state: any) => {
                          if (!state.sessionRuntimeGuidanceState) {
                            state.sessionRuntimeGuidanceState = {};
                          }
                          const prev = state.sessionRuntimeGuidanceState[sessionId] || {
                            pendingCount: 0,
                            appliedCount: 0,
                            lastGuidanceAt: guidanceAt,
                            lastAppliedAt: null,
                            items: [],
                          };
                          const pendingFromResponse = Number(
                            Number.isFinite(Number(response?.pending_count))
                              ? Number(response?.pending_count)
                              : Number.NaN
                          );
                          const responseGuidanceId = String(response?.guidance_id || '').trim();
                          const guidanceId = responseGuidanceId || optimisticGuidanceId;
                          const nextStatus = response?.status === 'applied'
                            ? 'applied'
                            : (response?.status === 'dropped' ? 'dropped' : 'queued');
                          const prevItems = Array.isArray(prev.items) ? [...prev.items] : [];
                          const existingIndex = prevItems.findIndex((item: any) => item.guidanceId === optimisticGuidanceId || item.guidanceId === guidanceId);
                          if (existingIndex >= 0) {
                            prevItems[existingIndex] = {
                              ...prevItems[existingIndex],
                              guidanceId,
                              turnId,
                              content: prevItems[existingIndex]?.content || trimmedContent,
                              status: nextStatus,
                            };
                          } else {
                            prevItems.unshift({
                              guidanceId,
                              turnId,
                              content: trimmedContent,
                              status: nextStatus,
                              createdAt: guidanceAt,
                              appliedAt: null,
                            });
                          }
                          state.sessionRuntimeGuidanceState[sessionId] = {
                            ...prev,
                            pendingCount: Number.isFinite(pendingFromResponse)
                              ? Math.max(0, pendingFromResponse)
                              : prev.pendingCount,
                            lastGuidanceAt: guidanceAt,
                            items: prevItems.slice(0, maxItems),
                          };
                        });
                        return {
                          success: response?.success === true,
                          guidanceId: response?.guidance_id,
                          status: response?.status,
                          pendingCount: response?.pending_count,
                          turnId: response?.turn_id,
                        };
                      } catch (error) {
                        set((state: any) => {
                          if (!state.sessionRuntimeGuidanceState) {
                            state.sessionRuntimeGuidanceState = {};
                          }
                          const prev = state.sessionRuntimeGuidanceState[sessionId] || {
                            pendingCount: 0,
                            appliedCount: 0,
                            lastGuidanceAt: null,
                            lastAppliedAt: null,
                            items: [],
                          };
                          const prevItems = Array.isArray(prev.items) ? prev.items : [];
                          state.sessionRuntimeGuidanceState[sessionId] = {
                            ...prev,
                            pendingCount: Math.max(0, Number(prev.pendingCount || 0) - 1),
                            items: prevItems.filter((item: any) => item.guidanceId !== optimisticGuidanceId),
                          };
                        });
                        throw error;
                      }
                    },
                    ...createStreamingActions({ set, get, client }),
                    setTaskReviewPanel: (panel: ChatState['taskReviewPanel']) => {
                        set((state: any) => {
                            state.taskReviewPanel = panel;
                        });
                    },
                    upsertTaskReviewPanel: (panel: TaskReviewPanelState) => {
                        if (!panel || !panel.reviewId || !panel.sessionId) {
                            return;
                        }
                        set((state: any) => {
                            const sessionId = panel.sessionId;
                            const panels = Array.isArray(state.taskReviewPanelsBySession?.[sessionId])
                                ? state.taskReviewPanelsBySession[sessionId]
                                : [];
                            const index = panels.findIndex((item: any) => item.reviewId === panel.reviewId);
                            if (index >= 0) {
                                panels[index] = panel;
                            } else {
                                panels.push(panel);
                            }
                            state.taskReviewPanelsBySession[sessionId] = panels;
                            if (state.currentSessionId === sessionId) {
                                state.taskReviewPanel = panels[0] || panel;
                            }
                        });
                    },
                    removeTaskReviewPanel: (reviewId: string, sessionId?: string) => {
                        if (!reviewId) {
                            return;
                        }
                        set((state: any) => {
                            const candidates = sessionId
                                ? [sessionId]
                                : Object.keys(state.taskReviewPanelsBySession || {});
                            for (const sid of candidates) {
                                const panels = state.taskReviewPanelsBySession?.[sid];
                                if (!Array.isArray(panels) || panels.length === 0) {
                                    continue;
                                }
                                const nextPanels = panels.filter((item: any) => item.reviewId !== reviewId);
                                if (nextPanels.length > 0) {
                                    state.taskReviewPanelsBySession[sid] = nextPanels;
                                } else {
                                    delete state.taskReviewPanelsBySession[sid];
                                }
                                if (state.currentSessionId === sid) {
                                    state.taskReviewPanel = nextPanels[0] || null;
                                }
                                break;
                            }
                        });
                    },
                    setUiPromptPanel: (panel: ChatState['uiPromptPanel']) => {
                        set((state: any) => {
                            state.uiPromptPanel = panel;
                        });
                    },
                    upsertUiPromptPanel: (panel: UiPromptPanelState) => {
                        if (!panel || !panel.promptId || !panel.sessionId) {
                            return;
                        }
                        set((state: any) => {
                            const sessionId = panel.sessionId;
                            const panels = Array.isArray(state.uiPromptPanelsBySession?.[sessionId])
                                ? state.uiPromptPanelsBySession[sessionId]
                                : [];
                            const index = panels.findIndex((item: any) => item.promptId === panel.promptId);
                            if (index >= 0) {
                                panels[index] = panel;
                            } else {
                                panels.push(panel);
                            }
                            state.uiPromptPanelsBySession[sessionId] = panels;
                            if (state.currentSessionId === sessionId) {
                                state.uiPromptPanel = panels[0] || panel;
                            }
                        });
                    },
                    removeUiPromptPanel: (promptId: string, sessionId?: string) => {
                        if (!promptId) {
                            return;
                        }
                        set((state: any) => {
                            const candidates = sessionId
                                ? [sessionId]
                                : Object.keys(state.uiPromptPanelsBySession || {});
                            for (const sid of candidates) {
                                const panels = state.uiPromptPanelsBySession?.[sid];
                                if (!Array.isArray(panels) || panels.length === 0) {
                                    continue;
                                }
                                const nextPanels = panels.filter((item: any) => item.promptId !== promptId);
                                if (nextPanels.length > 0) {
                                    state.uiPromptPanelsBySession[sid] = nextPanels;
                                } else {
                                    delete state.uiPromptPanelsBySession[sid];
                                }
                                if (state.currentSessionId === sid) {
                                    state.uiPromptPanel = nextPanels[0] || null;
                                }
                                break;
                            }
                        });
                    },
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
                    name: 'chat-store-with-backend',
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
