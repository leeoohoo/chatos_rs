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
import { createConversationControlActions } from './actions/conversationControl';
import { createAgentActions } from './actions/agents';
import { createSystemContextActions } from './actions/systemContexts';
import { createUiActions } from './actions/ui';
import { handleStreamEvent } from './actions/sendMessage/streamEventHandler';
import { finalizeStreamingSessionState } from './actions/sendMessage/sessionState';
import { createStreamingMessageStateHelpers } from './actions/sendMessage/streamingState';
import { rollbackFailedSendMessage } from './actions/sendMessage/failureState';
import type { StreamingMessage } from './actions/sendMessage/types';
import {
  normalizeImConversationMessage,
  normalizeRawMessages,
} from './helpers/messageNormalization';
import { readSessionImConversationId } from './helpers/sessionRuntime';
import { applyTurnProcessCache } from './helpers/messages';
import {
  ensureSessionTurnMaps,
  mergeMessagesWithStreamingDraft,
} from './actions/messagesState';
import { writeSessionMessagesCache } from './actions/sessionsUtils';
import { debugLog } from '@/lib/utils';
import {
  toTaskReviewPanelFromImActionRequest,
  toUiPromptPanelFromImActionRequest,
} from '../../components/chatInterface/helpers';
import type {
  ImConversationActionRequestResponse,
  ImConversationResponse,
  ImConversationMessageResponse,
  ImConversationRunResponse,
} from '../api/client/types';
import type {
  ChatActions,
  ChatState,
  ChatStoreGet,
  ChatStoreSet,
  ChatStoreConfig,
  ImConversationRuntimeState,
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
    let imEventsSocket: WebSocket | null = null;
    let imEventsReconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let imEventsReconnectAttempts = 0;
    let imEventsManualClose = false;
    let imEventsWsUrl: string | null = null;
    let imEventsBootstrapPromise: Promise<void> | null = null;
    type PendingSessionChatContext = {
      tempAssistantMessage: StreamingMessage;
      tempUserId: string | null;
      conversationTurnId: string;
      streamedTextRef: { value: string };
    };
    const pendingSessionChats = new Map<string, PendingSessionChatContext>();
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

    const normalizeId = (value: unknown): string => (
      typeof value === 'string' ? value.trim() : ''
    );

    const isBusyRunStatus = (status: unknown): boolean => {
      const normalizedStatus = normalizeId(status).toLowerCase();
      return normalizedStatus === 'queued'
        || normalizedStatus === 'pending'
        || normalizedStatus === 'running';
    };

    const clearSessionEventsReconnectTimer = () => {
      if (sessionEventsReconnectTimer) {
        clearTimeout(sessionEventsReconnectTimer);
        sessionEventsReconnectTimer = null;
      }
    };

    const clearPendingSessionChat = (sessionId?: string | null) => {
      const normalizedSessionId = typeof sessionId === 'string' ? sessionId.trim() : '';
      if (normalizedSessionId) {
        pendingSessionChats.delete(normalizedSessionId);
        return;
      }
      pendingSessionChats.clear();
    };

    const failPendingSessionChat = (sessionId: string, error: Error) => {
      const pendingChat = pendingSessionChats.get(sessionId);
      if (!pendingChat || !storeSet) {
        clearPendingSessionChat(sessionId);
        return;
      }

      rollbackFailedSendMessage({
        set: storeSet,
        currentSessionId: sessionId,
        tempAssistantId: pendingChat.tempAssistantMessage.id,
        tempAssistantMessage: pendingChat.tempAssistantMessage,
        streamedTextRef: pendingChat.streamedTextRef,
        error,
      });
      clearPendingSessionChat(sessionId);
    };

    const disconnectSessionEvents = () => {
      clearSessionEventsReconnectTimer();
      sessionEventsManualClose = true;
      sessionEventsReconnectAttempts = 0;
      if (sessionEventsSessionId) {
        failPendingSessionChat(
          sessionEventsSessionId,
          new Error('Session websocket disconnected'),
        );
      }
      if (sessionEventsSocket) {
        sessionEventsSocket.close();
        sessionEventsSocket = null;
      }
      sessionEventsSessionId = null;
    };

    const mergeSessionMessageState = (state: any, sessionId: string, normalizedMessage: any) => {
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
    };

    const upsertTaskReviewPanelState = (state: any, panel: TaskReviewPanelState) => {
      if (!panel?.reviewId || !panel?.sessionId) {
        return;
      }
      const sessionId = panel.sessionId;
      const panels = Array.isArray(state.taskReviewPanelsBySession?.[sessionId])
        ? state.taskReviewPanelsBySession[sessionId]
        : [];
      const index = panels.findIndex((item: any) => item.reviewId === panel.reviewId);
      if (index >= 0) {
        panels[index] = {
          ...panels[index],
          ...panel,
        };
      } else {
        panels.push(panel);
      }
      state.taskReviewPanelsBySession[sessionId] = panels;
      if (state.currentSessionId === sessionId) {
        state.taskReviewPanel = panels[0] || panel;
      }
    };

    const removeTaskReviewPanelState = (state: any, reviewId: string, sessionId?: string) => {
      const normalizedReviewId = typeof reviewId === 'string' ? reviewId.trim() : '';
      if (!normalizedReviewId) {
        return;
      }
      const candidates = sessionId
        ? [sessionId]
        : Object.keys(state.taskReviewPanelsBySession || {});
      for (const sid of candidates) {
        const panels = state.taskReviewPanelsBySession?.[sid];
        if (!Array.isArray(panels) || panels.length === 0) {
          continue;
        }
        const nextPanels = panels.filter((item: any) => item.reviewId !== normalizedReviewId);
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
    };

    const upsertUiPromptPanelState = (state: any, panel: UiPromptPanelState) => {
      if (!panel?.promptId || !panel?.sessionId) {
        return;
      }
      const sessionId = panel.sessionId;
      const panels = Array.isArray(state.uiPromptPanelsBySession?.[sessionId])
        ? state.uiPromptPanelsBySession[sessionId]
        : [];
      const index = panels.findIndex((item: any) => item.promptId === panel.promptId);
      if (index >= 0) {
        panels[index] = {
          ...panels[index],
          ...panel,
        };
      } else {
        panels.push(panel);
      }
      state.uiPromptPanelsBySession[sessionId] = panels;
      if (state.currentSessionId === sessionId) {
        state.uiPromptPanel = panels[0] || panel;
      }
    };

    const removeUiPromptPanelState = (state: any, promptId: string, sessionId?: string) => {
      const normalizedPromptId = typeof promptId === 'string' ? promptId.trim() : '';
      if (!normalizedPromptId) {
        return;
      }
      const candidates = sessionId
        ? [sessionId]
        : Object.keys(state.uiPromptPanelsBySession || {});
      for (const sid of candidates) {
        const panels = state.uiPromptPanelsBySession?.[sid];
        if (!Array.isArray(panels) || panels.length === 0) {
          continue;
        }
        const nextPanels = panels.filter((item: any) => item.promptId !== normalizedPromptId);
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
    };

    const resolveImFallbackTurnId = (
      actionRequest: ImConversationActionRequestResponse,
    ): string => {
      const runId = normalizeId(actionRequest?.run_id);
      if (runId) {
        return `im-run-${runId}`;
      }
      const actionRequestId = normalizeId(actionRequest?.id);
      return actionRequestId ? `im-action-${actionRequestId}` : '';
    };

    const upsertImConversationRuntimeState = (
      state: any,
      conversationId: string,
      updates: Partial<ImConversationRuntimeState>,
    ) => {
      const normalizedConversationId = normalizeId(conversationId);
      if (!normalizedConversationId) {
        return;
      }
      const previous = state.imConversationRuntimeByConversationId?.[normalizedConversationId] || {
        busy: false,
        unreadCount: 0,
        latestRunStatus: null,
        lastMessagePreview: null,
        lastMessageAt: null,
      };
      state.imConversationRuntimeByConversationId[normalizedConversationId] = {
        ...previous,
        ...updates,
      };
    };

    const upsertImConversationListState = (
      state: any,
      conversation: Partial<ImConversationResponse> & { id?: string },
    ) => {
      const conversationId = normalizeId(conversation?.id);
      if (!conversationId) {
        return;
      }
      const currentList = Array.isArray(state.imConversations) ? state.imConversations : [];
      const index = currentList.findIndex((item: any) => normalizeId(item?.id) === conversationId);
      if (index >= 0) {
        currentList[index] = {
          ...currentList[index],
          ...conversation,
          id: conversationId,
        };
      } else {
        currentList.unshift({
          id: conversationId,
          owner_user_id: '',
          contact_id: '',
          ...conversation,
        });
      }
    };

    const applyImConversationRuntimeState = (
      state: any,
      conversation: ImConversationResponse,
    ) => {
      const conversationId = normalizeId(conversation?.id);
      if (!conversationId) {
        return;
      }

      upsertImConversationRuntimeState(state, conversationId, {
        unreadCount: Number(conversation?.unread_count || 0),
        lastMessagePreview: typeof conversation?.last_message_preview === 'string'
          ? conversation.last_message_preview
          : null,
        lastMessageAt: typeof conversation?.last_message_at === 'string'
          ? conversation.last_message_at
          : (typeof conversation?.updated_at === 'string' ? conversation.updated_at : null),
      });
    };

    const applyImRunRuntimeState = (
      state: any,
      conversationId: string,
      run: ImConversationRunResponse,
    ) => {
      const latestRunStatus = normalizeId(run?.status) || null;
      upsertImConversationRuntimeState(state, conversationId, {
        busy: isBusyRunStatus(latestRunStatus),
        latestRunStatus,
      });
    };

    const replaceImActionPanelsState = (
      state: any,
      sessionId: string,
      actionRequests: ImConversationActionRequestResponse[],
    ) => {
      const nextTaskPanels = (Array.isArray(actionRequests) ? actionRequests : [])
        .filter((item) => normalizeId(item?.status).toLowerCase() === 'pending')
        .map((item) => toTaskReviewPanelFromImActionRequest(
          item,
          sessionId,
          resolveImFallbackTurnId(item),
        ))
        .filter(Boolean) as TaskReviewPanelState[];
      const nextUiPromptPanels = (Array.isArray(actionRequests) ? actionRequests : [])
        .filter((item) => normalizeId(item?.status).toLowerCase() === 'pending')
        .map((item) => toUiPromptPanelFromImActionRequest(
          item,
          sessionId,
          resolveImFallbackTurnId(item),
        ))
        .filter(Boolean) as UiPromptPanelState[];

      const preservedTaskPanels = Array.isArray(state.taskReviewPanelsBySession?.[sessionId])
        ? state.taskReviewPanelsBySession[sessionId].filter((panel: any) => panel?.source !== 'im')
        : [];
      const preservedUiPromptPanels = Array.isArray(state.uiPromptPanelsBySession?.[sessionId])
        ? state.uiPromptPanelsBySession[sessionId].filter((panel: any) => panel?.source !== 'im')
        : [];

      const mergedTaskPanels = [...preservedTaskPanels, ...nextTaskPanels];
      const mergedUiPromptPanels = [...preservedUiPromptPanels, ...nextUiPromptPanels];

      if (mergedTaskPanels.length > 0) {
        state.taskReviewPanelsBySession[sessionId] = mergedTaskPanels;
      } else {
        delete state.taskReviewPanelsBySession[sessionId];
      }
      if (mergedUiPromptPanels.length > 0) {
        state.uiPromptPanelsBySession[sessionId] = mergedUiPromptPanels;
      } else {
        delete state.uiPromptPanelsBySession[sessionId];
      }

      if (state.currentSessionId === sessionId) {
        state.taskReviewPanel = mergedTaskPanels[0] || null;
        state.uiPromptPanel = mergedUiPromptPanels[0] || null;
      }
    };

    const resolveRuntimeSessionIdByConversationId = (conversationId: string): string | null => {
      const normalizedConversationId = normalizeId(conversationId);
      if (!normalizedConversationId || !storeGet) {
        return null;
      }
      const state = storeGet();
      const matched = (state.sessions || []).find((session: any) => (
        readSessionImConversationId(session?.metadata) === normalizedConversationId
      ));
      return matched?.id || null;
    };

    const applyImActionRequestState = (
      state: any,
      sessionId: string,
      actionRequest: ImConversationActionRequestResponse,
    ) => {
      const fallbackTurnId = resolveImFallbackTurnId(actionRequest);
      const normalizedStatus = String(actionRequest?.status || '').trim().toLowerCase();
      const isPending = normalizedStatus === 'pending';

      const taskReviewPanel = toTaskReviewPanelFromImActionRequest(
        actionRequest,
        sessionId,
        fallbackTurnId,
      );
      if (taskReviewPanel) {
        if (isPending) {
          upsertTaskReviewPanelState(state, taskReviewPanel);
        } else {
          removeTaskReviewPanelState(state, taskReviewPanel.reviewId, sessionId);
        }
      }

      const uiPromptPanel = toUiPromptPanelFromImActionRequest(
        actionRequest,
        sessionId,
        fallbackTurnId,
      );
      if (uiPromptPanel) {
        if (isPending) {
          upsertUiPromptPanelState(state, uiPromptPanel);
        } else {
          removeUiPromptPanelState(state, uiPromptPanel.promptId, sessionId);
        }
      }
    };

    const handleIncomingSessionEvent = (sessionId: string, payload: any) => {
      const set = storeSet;
      const get = storeGet;
      if (!set || !get) {
        return;
      }
      if (payload && sessionChatStreamEventTypes.has(String(payload.type || ''))) {
        const pendingChat = pendingSessionChats.get(sessionId);
        if (!pendingChat) {
          return;
        }
        const helpers = createStreamingMessageStateHelpers({
          set,
          currentSessionId: sessionId,
          tempAssistantMessage: pendingChat.tempAssistantMessage,
          tempUserId: pendingChat.tempUserId,
          conversationTurnId: pendingChat.conversationTurnId,
          streamedTextRef: pendingChat.streamedTextRef,
        });

        try {
          const eventResult = handleStreamEvent({
            parsed: payload,
            set,
            currentSessionId: sessionId,
            conversationTurnId: pendingChat.conversationTurnId,
            tempAssistantMessageId: pendingChat.tempAssistantMessage.id,
            streamedTextRef: pendingChat.streamedTextRef,
            helpers,
          });
          if (eventResult.sawDone || sessionChatTerminalEventTypes.has(String(payload.type || ''))) {
            set((state: any) => {
              finalizeStreamingSessionState(state, {
                sessionId,
                assistantMessageId: pendingChat.tempAssistantMessage.id,
                sawDone: eventResult.sawDone,
              });
            });
            clearPendingSessionChat(sessionId);
          }
        } catch (error) {
          failPendingSessionChat(
            sessionId,
            error instanceof Error ? error : new Error(String(error)),
          );
        }
        return;
      }
      if (!payload || typeof payload.type !== 'string') {
        return;
      }

      if (payload.type !== 'task_execution.notice' || !payload.message) {
        return;
      }

      const normalizedMessage = normalizeRawMessages([payload.message], sessionId)[0];
      if (!normalizedMessage) {
        return;
      }

      set((state: any) => {
        mergeSessionMessageState(state, sessionId, normalizedMessage);
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

    const startSessionChatViaWs = async (
      sessionId: string,
      content: string,
      modelConfig: StreamChatModelConfigPayload,
      streamUserId?: string,
      attachments?: StreamChatAttachmentPayload[],
      reasoningEnabled?: boolean,
      options?: StreamChatOptions,
      pendingContext?: PendingSessionChatContext,
    ): Promise<void> => {
      if (pendingSessionChats.has(sessionId)) {
        throw new Error('A chat stream is already active on the current session websocket');
      }

      const ws = await waitForSessionEventsOpen(sessionId);
      if (pendingContext) {
        pendingSessionChats.set(sessionId, pendingContext);
      }
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
        clearPendingSessionChat(sessionId);
        throw error instanceof Error ? error : new Error(String(error));
      }
    };

    const abortSessionChatViaWs = async (sessionId: string): Promise<boolean> => {
      if (
        !pendingSessionChats.has(sessionId)
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
        if (pendingSessionChats.has(sessionId)) {
          failPendingSessionChat(sessionId, new Error('Session websocket closed during chat stream'));
        }
        sessionEventsSocket = null;
        if (shouldReconnect) {
          scheduleSessionEventsReconnect(sessionId);
        }
      };
    };

    const clearImEventsReconnectTimer = () => {
      if (imEventsReconnectTimer) {
        clearTimeout(imEventsReconnectTimer);
        imEventsReconnectTimer = null;
      }
    };

    const buildImEventsSocketUrl = (rawWsUrl: string, accessToken?: string | null): string => {
      const wsUrl = new URL(rawWsUrl, window.location.origin);
      const token = normalizeId(accessToken);
      if (token) {
        wsUrl.searchParams.set('access_token', token);
      }
      return wsUrl.toString();
    };

    const bootstrapImConversationState = async (targetSessionId?: string | null): Promise<void> => {
      if (!storeGet || !storeSet) {
        return;
      }
      if (imEventsBootstrapPromise) {
        await imEventsBootstrapPromise;
        return;
      }

      imEventsBootstrapPromise = (async () => {
        const state = storeGet?.();
        if (!state) {
          return;
        }

        const refs = (state.sessions || [])
          .map((session: any) => ({
            sessionId: normalizeId(session?.id),
            conversationId: readSessionImConversationId(session?.metadata),
          }))
          .filter((item) => item.sessionId && item.conversationId)
          .filter((item) => !targetSessionId || item.sessionId === targetSessionId) as Array<{
            sessionId: string;
            conversationId: string;
          }>;

        if (refs.length === 0) {
          return;
        }

        const conversations = await client.getImConversations().catch(() => []);
        const conversationMap = new Map(
          (Array.isArray(conversations) ? conversations : [])
            .map((conversation) => [normalizeId(conversation?.id), conversation] as const)
            .filter(([conversationId]) => Boolean(conversationId)),
        );

        const runEntries = await Promise.all(refs.map(async (ref) => {
          const runs = await client.getImConversationRuns(ref.conversationId).catch(() => []);
          const latestRun = Array.isArray(runs) ? (runs[0] || null) : null;
          return [ref.conversationId, latestRun] as const;
        }));
        const actionEntries = await Promise.all(refs.map(async (ref) => {
          const actionRequests = await client
            .getImConversationActionRequests(ref.conversationId)
            .catch(() => []);
          return [
            ref.sessionId,
            Array.isArray(actionRequests) ? actionRequests : [],
          ] as const;
        }));

        const latestRunByConversationId = new Map(runEntries);
        const actionRequestsBySessionId = new Map(actionEntries);

        storeSet((draft: any) => {
          refs.forEach((ref) => {
            const conversation = conversationMap.get(ref.conversationId);
            if (conversation) {
              applyImConversationRuntimeState(draft, conversation);
            }

            const latestRun = latestRunByConversationId.get(ref.conversationId);
            if (latestRun) {
              applyImRunRuntimeState(draft, ref.conversationId, latestRun);
            }

            replaceImActionPanelsState(
              draft,
              ref.sessionId,
              actionRequestsBySessionId.get(ref.sessionId) || [],
            );
          });
        });
      })().finally(() => {
        imEventsBootstrapPromise = null;
      });

      await imEventsBootstrapPromise;
    };

    const handleIncomingImEvent = (payload: any) => {
      const set = storeSet;
      const get = storeGet;
      if (!set || !get || !payload || typeof payload.type !== 'string') {
        return;
      }

      if (payload.type === 'im.connected') {
        return;
      }

      if (
        (payload.type === 'im.conversation.created' || payload.type === 'im.conversation.updated')
        && payload.conversation
      ) {
        set((state: any) => {
          const conversation = payload.conversation as ImConversationResponse;
          applyImConversationRuntimeState(state, conversation);
          upsertImConversationListState(state, conversation);
        });
        return;
      }

      if (payload.type === 'im.message.created' && payload.message) {
        const rawMessage = payload.message as ImConversationMessageResponse;
        const conversationId = normalizeId(payload.conversation_id ?? rawMessage?.conversation_id);
        const runtimeSessionId = resolveRuntimeSessionIdByConversationId(conversationId);
        const normalizedMessage = runtimeSessionId
          ? normalizeImConversationMessage(rawMessage, runtimeSessionId)
          : null;

        set((state: any) => {
          const previousRuntime = state.imConversationRuntimeByConversationId?.[conversationId];
          const nextUnreadCount = normalizeId(rawMessage?.sender_type).toLowerCase() === 'user'
            ? Number(previousRuntime?.unreadCount || 0)
            : Number(previousRuntime?.unreadCount || 0) + 1;
          upsertImConversationRuntimeState(state, conversationId, {
            lastMessagePreview: typeof rawMessage?.content === 'string' ? rawMessage.content : null,
            lastMessageAt: typeof rawMessage?.updated_at === 'string'
              ? rawMessage.updated_at
              : (typeof rawMessage?.created_at === 'string' ? rawMessage.created_at : null),
            unreadCount: nextUnreadCount,
          });
          upsertImConversationListState(state, {
            id: conversationId,
            last_message_preview: typeof rawMessage?.content === 'string' ? rawMessage.content : null,
            last_message_at: typeof rawMessage?.updated_at === 'string'
              ? rawMessage.updated_at
              : (typeof rawMessage?.created_at === 'string' ? rawMessage.created_at : null),
            unread_count: nextUnreadCount,
          });
          if (runtimeSessionId && normalizedMessage) {
            mergeSessionMessageState(state, runtimeSessionId, normalizedMessage);
          }
        });

        const currentState = get();
        if (runtimeSessionId && currentState.currentSessionId === runtimeSessionId) {
          writeSessionMessagesCache(runtimeSessionId, currentState.messages || []);
          if (normalizeId(rawMessage?.sender_type).toLowerCase() !== 'user') {
            void client.markImConversationRead(conversationId).catch(() => {});
            set((state: any) => {
              upsertImConversationRuntimeState(state, conversationId, {
                unreadCount: 0,
              });
              upsertImConversationListState(state, {
                id: conversationId,
                unread_count: 0,
              });
            });
          }
        }
        return;
      }

      if (
        (payload.type === 'im.action_request.created' || payload.type === 'im.action_request.updated')
        && payload.action_request
      ) {
        const actionRequest = payload.action_request as ImConversationActionRequestResponse;
        const conversationId = normalizeId(payload.conversation_id ?? actionRequest?.conversation_id);
        const runtimeSessionId = resolveRuntimeSessionIdByConversationId(conversationId);
        if (!runtimeSessionId) {
          return;
        }
        set((state: any) => {
          applyImActionRequestState(state, runtimeSessionId, actionRequest);
        });
        return;
      }

      if (
        (payload.type === 'im.run.created' || payload.type === 'im.run.updated')
        && payload.run
      ) {
        const run = payload.run as ImConversationRunResponse;
        const conversationId = normalizeId(payload.conversation_id ?? run?.conversation_id);
        set((state: any) => {
          applyImRunRuntimeState(state, conversationId, run);
        });
      }
    };

    const scheduleImEventsReconnect = () => {
      clearImEventsReconnectTimer();
      const delay = Math.min(1000 * 2 ** imEventsReconnectAttempts, 15000);
      imEventsReconnectAttempts += 1;
      imEventsReconnectTimer = setTimeout(() => {
        void connectImEvents();
      }, delay);
    };

    const connectImEvents = async (): Promise<void> => {
      if (typeof window === 'undefined') {
        return;
      }
      const accessToken = normalizeId(client.getAccessToken());
      if (!accessToken) {
        return;
      }
      if (
        imEventsSocket
        && (imEventsSocket.readyState === WebSocket.OPEN
          || imEventsSocket.readyState === WebSocket.CONNECTING)
      ) {
        return;
      }

      clearImEventsReconnectTimer();
      imEventsManualClose = false;

      if (!imEventsWsUrl) {
        const meta = await client.getImWsMeta().catch(
          (): { ws_url?: string | null } => ({}),
        );
        const rawWsUrl = normalizeId(meta?.ws_url);
        if (!rawWsUrl) {
          return;
        }
        imEventsWsUrl = rawWsUrl;
      }

      if (imEventsSocket) {
        imEventsSocket.close();
        imEventsSocket = null;
      }

      const socketUrl = buildImEventsSocketUrl(imEventsWsUrl, accessToken);
      const ws = new WebSocket(socketUrl);
      imEventsSocket = ws;

      ws.onopen = () => {
        imEventsReconnectAttempts = 0;
        debugLog('[Store] IM ws connected');
        void bootstrapImConversationState();
      };
      ws.onmessage = (event) => {
        try {
          const parsed = JSON.parse(String(event.data || '{}'));
          handleIncomingImEvent(parsed);
        } catch (error) {
          console.error('Failed to parse IM ws event:', error);
        }
      };
      ws.onerror = () => {
        debugLog('[Store] IM ws error');
      };
      ws.onclose = () => {
        const shouldReconnect = !imEventsManualClose;
        imEventsSocket = null;
        if (shouldReconnect) {
          scheduleImEventsReconnect();
        }
      };
    };

    client.onAccessTokenRefresh(() => {
      if (sessionEventsSessionId) {
        connectSessionEvents(sessionEventsSessionId);
      }
      void connectImEvents();
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
                    imConversations: [],
                    imConversationRuntimeByConversationId: {},
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

                    // 会话/项目/消息/会话控制/UI 操作（拆分到独立模块）
                    ...createContactActions({ set, get, client, getUserIdParam }),
                    ...createSessionActions({
                      set,
                      get,
                      client,
                      getSessionParams,
                      customUserId,
                      customProjectId,
                      onSessionActivated: (sessionId) => {
                        connectSessionEvents(sessionId);
                        void connectImEvents();
                        void bootstrapImConversationState(sessionId);
                      },
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
                      startSessionChat: startSessionChatViaWs,
                    }),
                    ...createConversationControlActions({
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
