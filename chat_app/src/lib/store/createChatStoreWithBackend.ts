import { createWithEqualityFn } from 'zustand/traditional';
import {immer} from 'zustand/middleware/immer';
import {persist} from 'zustand/middleware';
import {apiClient} from '../api/client';
import type ApiClient from '../api/client';
import { buildWsUrl } from '../api/client/ws';
import {createSendMessageHandler} from './actions/sendMessage';
import type {
  StreamChatAttachmentPayload,
  StreamChatModelConfigPayload,
  StreamChatOptions,
} from '../api/client/types';
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
import { createRuntimeGuidanceActions } from './actions/runtimeGuidance';
import { createStreamingActions } from './actions/streaming';
import { createAgentActions } from './actions/agents';
import { createSystemContextActions } from './actions/systemContexts';
import { createUiActions } from './actions/ui';
import { normalizeRawMessages } from './helpers/messageNormalization';
import { applyTurnProcessCache } from './helpers/messages';
import {
  ensureSessionTurnMaps,
  mergeMessagesWithStreamingDraft,
} from './actions/messagesState';
import { writeSessionMessagesCache } from './actions/sessionsUtils';
import { debugLog } from '@/lib/utils';
import type {
  ChatActions,
  ChatState,
  ChatStoreGet,
  ChatStoreSet,
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
    let storeSet: ChatStoreSet | null = null;
    let storeGet: ChatStoreGet | null = null;
    let sessionEventsSocket: WebSocket | null = null;
    let sessionEventsSessionId: string | null = null;
    let sessionEventsReconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let sessionEventsReconnectAttempts = 0;
    let sessionEventsManualClose = false;
    let pendingSessionChatController: ReadableStreamDefaultController<Uint8Array> | null = null;
    let pendingSessionChatSessionId: string | null = null;
    const textEncoder = new TextEncoder();
    const sessionChatStreamEventTypes = new Set([
      'start',
      'chunk',
      'thinking',
      'content',
      'tools_start',
      'tools_stream',
      'tools_end',
      'context_summarized_start',
      'context_summarized_stream',
      'context_summarized_end',
      'runtime_guidance_queued',
      'runtime_guidance_applied',
      'complete',
      'done',
      'cancelled',
      'error',
    ]);
    const sessionChatTerminalEventTypes = new Set(['complete', 'done', 'cancelled', 'error']);
    
    // 用户 ID 由登录态注入；缺失时不再回退到硬编码默认值
    const userId = customUserId || '';
    
    // 获取userId的统一函数
    const getUserIdParam = () => userId;

    const clearSessionEventsReconnectTimer = () => {
      if (sessionEventsReconnectTimer) {
        clearTimeout(sessionEventsReconnectTimer);
        sessionEventsReconnectTimer = null;
      }
    };

    const clearPendingSessionChat = () => {
      pendingSessionChatController = null;
      pendingSessionChatSessionId = null;
    };

    const finalizePendingSessionChat = (error?: Error) => {
      const controller = pendingSessionChatController;
      if (!controller) {
        clearPendingSessionChat();
        return;
      }
      clearPendingSessionChat();
      try {
        if (error) {
          controller.error(error);
        } else {
          controller.close();
        }
      } catch {
        // ignore stream finalization errors
      }
    };

    const disconnectSessionEvents = () => {
      clearSessionEventsReconnectTimer();
      sessionEventsManualClose = true;
      sessionEventsReconnectAttempts = 0;
      finalizePendingSessionChat(new Error('Session websocket disconnected'));
      if (sessionEventsSocket) {
        sessionEventsSocket.close();
        sessionEventsSocket = null;
      }
      sessionEventsSessionId = null;
    };

    const handleIncomingSessionEvent = (sessionId: string, payload: any) => {
      const set = storeSet;
      const get = storeGet;
      if (!set || !get) {
        return;
      }
      if (payload && sessionChatStreamEventTypes.has(String(payload.type || ''))) {
        if (pendingSessionChatController && pendingSessionChatSessionId === sessionId) {
          pendingSessionChatController.enqueue(
            textEncoder.encode(`data: ${JSON.stringify(payload)}\n\n`),
          );
          if (sessionChatTerminalEventTypes.has(String(payload.type || ''))) {
            finalizePendingSessionChat();
          }
        }
        return;
      }
      if (!payload || payload.type !== 'task_execution.notice' || !payload.message) {
        return;
      }

      const normalizedMessage = normalizeRawMessages([payload.message], sessionId)[0];
      if (!normalizedMessage) {
        return;
      }

      set((state: any) => {
        ensureSessionTurnMaps(state, sessionId);

        const currentMessages = state.currentSessionId === sessionId
          ? Array.isArray(state.messages) ? [...state.messages] : []
          : [];
        const existingIndex = currentMessages.findIndex((item: any) => item?.id === normalizedMessage.id);
        if (existingIndex >= 0) {
          currentMessages[existingIndex] = {
            ...currentMessages[existingIndex],
            ...normalizedMessage,
          };
        } else {
          currentMessages.push(normalizedMessage);
        }
        currentMessages.sort((left: any, right: any) => (
          new Date(left?.createdAt || 0).getTime() - new Date(right?.createdAt || 0).getTime()
        ));

        const mergedMessages = mergeMessagesWithStreamingDraft(state, sessionId, currentMessages);
        state.messages = applyTurnProcessCache(
          mergedMessages,
          state.sessionTurnProcessCache?.[sessionId],
          state.sessionTurnProcessState?.[sessionId],
        );

        const nextUpdatedAt = normalizedMessage.createdAt || new Date();
        const sessionIndex = state.sessions.findIndex((item: any) => item?.id === sessionId);
        if (sessionIndex >= 0) {
          state.sessions[sessionIndex] = {
            ...state.sessions[sessionIndex],
            updatedAt: nextUpdatedAt,
          };
        }
        if (state.currentSession?.id === sessionId) {
          state.currentSession = {
            ...(state.currentSession || {}),
            updatedAt: nextUpdatedAt,
          };
        }
      });

      const state = get();
      if (state.currentSessionId === sessionId) {
        writeSessionMessagesCache(sessionId, state.messages || []);
      }
      debugLog('[Store] received session ws event', {
        sessionId,
        eventType: payload?.event,
        messageId: normalizedMessage.id,
      });
    };

    const scheduleSessionEventsReconnect = (sessionId: string) => {
      clearSessionEventsReconnectTimer();
      const delay = Math.min(1000 * 2 ** sessionEventsReconnectAttempts, 15000);
      sessionEventsReconnectAttempts += 1;
      sessionEventsReconnectTimer = setTimeout(() => {
        connectSessionEvents(sessionId);
      }, delay);
    };

    const waitForSessionEventsOpen = (sessionId: string): Promise<WebSocket> => new Promise((resolve, reject) => {
      connectSessionEvents(sessionId);
      const ws = sessionEventsSocket;
      if (!ws || sessionEventsSessionId !== sessionId) {
        reject(new Error('Session websocket is not available'));
        return;
      }
      if (ws.readyState === WebSocket.OPEN) {
        resolve(ws);
        return;
      }
      if (ws.readyState !== WebSocket.CONNECTING) {
        reject(new Error('Session websocket is not connecting'));
        return;
      }

      const timeout = window.setTimeout(() => {
        cleanup();
        reject(new Error('Session websocket connection timed out'));
      }, 8000);

      const handleOpen = () => {
        cleanup();
        resolve(ws);
      };
      const handleClose = () => {
        cleanup();
        reject(new Error('Session websocket closed before ready'));
      };
      const handleError = () => {
        cleanup();
        reject(new Error('Session websocket failed to connect'));
      };
      const cleanup = () => {
        window.clearTimeout(timeout);
        ws.removeEventListener('open', handleOpen);
        ws.removeEventListener('close', handleClose);
        ws.removeEventListener('error', handleError);
      };

      ws.addEventListener('open', handleOpen);
      ws.addEventListener('close', handleClose);
      ws.addEventListener('error', handleError);
    });

    const streamChatViaSessionWs = async (
      sessionId: string,
      content: string,
      modelConfig: StreamChatModelConfigPayload,
      streamUserId?: string,
      attachments?: StreamChatAttachmentPayload[],
      reasoningEnabled?: boolean,
      options?: StreamChatOptions,
    ): Promise<ReadableStream> => {
      if (pendingSessionChatController) {
        throw new Error('A chat stream is already active on the current session websocket');
      }

      const ws = await waitForSessionEventsOpen(sessionId);
      return new ReadableStream<Uint8Array>({
        start(controller) {
          pendingSessionChatController = controller;
          pendingSessionChatSessionId = sessionId;
          try {
            ws.send(JSON.stringify({
              type: 'chat.send',
              request: {
                content,
                user_id: streamUserId,
                attachments: attachments || [],
                reasoning_enabled: reasoningEnabled,
                turn_id: options?.turnId,
                contact_agent_id: options?.contactAgentId || undefined,
                remote_connection_id: Object.prototype.hasOwnProperty.call(options || {}, 'remoteConnectionId')
                  ? (options?.remoteConnectionId ?? null)
                  : undefined,
                project_id: options?.projectId || undefined,
                project_root: options?.projectRoot || undefined,
                mcp_enabled: options?.mcpEnabled,
                enabled_mcp_ids: options?.enabledMcpIds || [],
                ai_model_config: {
                  provider: modelConfig.provider,
                  model_name: modelConfig.model_name,
                  temperature: modelConfig.temperature || 0.7,
                  thinking_level: modelConfig.thinking_level,
                  api_key: modelConfig.api_key,
                  base_url: modelConfig.base_url,
                  supports_images: modelConfig.supports_images === true,
                  supports_reasoning: modelConfig.supports_reasoning === true,
                  supports_responses: modelConfig.supports_responses === true,
                },
              },
            }));
          } catch (error) {
            clearPendingSessionChat();
            controller.error(error instanceof Error ? error : new Error(String(error)));
          }
        },
        cancel() {
          if (
            pendingSessionChatSessionId === sessionId
            && ws.readyState === WebSocket.OPEN
          ) {
            try {
              ws.send(JSON.stringify({ type: 'chat.stop' }));
            } catch {
              // ignore stop failures during cancel
            }
          }
          finalizePendingSessionChat();
        },
      });
    };

    const abortSessionChatViaWs = async (sessionId: string): Promise<boolean> => {
      if (
        pendingSessionChatSessionId !== sessionId
        || !sessionEventsSocket
        || sessionEventsSessionId !== sessionId
        || sessionEventsSocket.readyState !== WebSocket.OPEN
      ) {
        return false;
      }
      sessionEventsSocket.send(JSON.stringify({ type: 'chat.stop' }));
      return true;
    };

    const connectSessionEvents = (sessionId: string | null) => {
      const get = storeGet;
      if (!sessionId || typeof window === 'undefined' || !get) {
        disconnectSessionEvents();
        return;
      }

      const currentSessionId = get().currentSessionId;
      if (currentSessionId !== sessionId) {
        return;
      }

      if (
        sessionEventsSocket
        && sessionEventsSessionId === sessionId
        && (sessionEventsSocket.readyState === WebSocket.OPEN
          || sessionEventsSocket.readyState === WebSocket.CONNECTING)
      ) {
        return;
      }

      clearSessionEventsReconnectTimer();
      sessionEventsManualClose = false;
      if (sessionEventsSocket) {
        sessionEventsSocket.close();
        sessionEventsSocket = null;
      }

      sessionEventsSessionId = sessionId;
      const wsUrl = buildWsUrl(
        client.getBaseUrl(),
        `/sessions/${encodeURIComponent(sessionId)}/ws`,
        client.getAccessToken(),
      );
      const ws = new WebSocket(wsUrl);
      sessionEventsSocket = ws;

      ws.onopen = () => {
        sessionEventsReconnectAttempts = 0;
        debugLog('[Store] session ws connected', { sessionId });
      };
      ws.onmessage = (event) => {
        try {
          const parsed = JSON.parse(String(event.data || '{}'));
          handleIncomingSessionEvent(sessionId, parsed);
        } catch (error) {
          console.error('Failed to parse session ws event:', error);
        }
      };
      ws.onerror = () => {
        debugLog('[Store] session ws error', { sessionId });
      };
      ws.onclose = () => {
        const shouldReconnect = !sessionEventsManualClose && storeGet?.().currentSessionId === sessionId;
        if (pendingSessionChatSessionId === sessionId) {
          finalizePendingSessionChat(new Error('Session websocket closed during chat stream'));
        }
        sessionEventsSocket = null;
        if (shouldReconnect) {
          scheduleSessionEventsReconnect(sessionId);
        }
      };
    };

    client.onAccessTokenRefresh(() => {
      if (sessionEventsSessionId) {
        connectSessionEvents(sessionEventsSessionId);
      }
    });
    
    return createWithEqualityFn<ChatState & ChatActions>()(
        immer(
            persist(
                    (set, get) => {
                    storeSet = set as ChatStoreSet;
                    storeGet = get as ChatStoreGet;
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
                    ...createSessionActions({
                      set,
                      get,
                      client,
                      getSessionParams,
                      customUserId,
                      customProjectId,
                      onSessionActivated: connectSessionEvents,
                    }),
                    ...createProjectActions({ set, get, client, getUserIdParam }),
                    ...createTerminalActions({ set, get, client, getUserIdParam }),
                    ...createRemoteConnectionActions({ set, get, client, getUserIdParam }),
                    ...createMessageActions({ set, get, client }),
                    ...createRuntimeGuidanceActions({ set, client }),
                    sendMessage: createSendMessageHandler({
                      set,
                      get,
                      client,
                      getUserIdParam,
                      streamChat: streamChatViaSessionWs,
                    }),
                    ...createStreamingActions({
                      set,
                      get,
                      client,
                      abortSessionChat: abortSessionChatViaWs,
                    }),
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
