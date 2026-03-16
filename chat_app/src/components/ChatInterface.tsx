import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { shallow } from 'zustand/shallow';
import { useChatApiClientFromContext, useChatStoreSelector } from '../lib/store/ChatStoreContext';
import { MessageList } from './MessageList';
import { SessionList } from './SessionList';
import McpManager from './McpManager';
import AiModelManager from './AiModelManager';
import SystemContextEditor from './SystemContextEditor';
import UserSettingsPanel from './UserSettingsPanel';
import ProjectExplorer from './ProjectExplorer';
import TerminalView from './TerminalView';
import RemoteTerminalView from './RemoteTerminalView';
import RemoteSftpPanel from './RemoteSftpPanel';
// 搴旂敤寮圭獥绠＄悊鍣ㄧ敱 ApplicationsPanel 鐩存帴鎵挎媴
import ApplicationsPanel from './ApplicationsPanel';
import NotepadPanel from './NotepadPanel';
import ChatComposerPanel from './chatInterface/ChatComposerPanel';
import HeaderBar from './chatInterface/HeaderBar';
import SummaryPane from './chatInterface/SummaryPane';
import UiPromptHistoryDrawer from './chatInterface/UiPromptHistoryDrawer';
import {
  collectMessageToolCalls,
  formatSummaryCreatedAt,
  hasToolCallError,
  normalizeWorkbarSummary,
  normalizeWorkbarTask,
  normalizeUiPromptHistoryItem,
  selectLatestTurnTasks,
  shouldRefreshForTaskMutationToolCall,
  toUiPromptPanelFromRecord,
  extractTaskIdsFromToolCall,
} from './chatInterface/helpers';
import { usePanelActions } from './chatInterface/usePanelActions';
import { useWorkbarMutations } from './chatInterface/useWorkbarMutations';
import { readSessionRuntimeFromMetadata } from '../lib/store/helpers/sessionRuntime';
import type { UiPromptHistoryItem } from './chatInterface/types';
import type { SessionSummaryWorkbarItem, TaskWorkbarItem } from './TaskWorkbar';
import { apiClient as globalApiClient } from '../lib/api/client';
import { cn } from '../lib/utils';
import type { ChatInterfaceProps } from '../types';
import { useAuthStore } from '../lib/auth/authStore';

const SESSION_PAGE_SIZE = 30;
const WORKBAR_SUMMARY_PAGE_SIZE = 50;

const appendUniqueSummaries = (
  current: SessionSummaryWorkbarItem[],
  incoming: SessionSummaryWorkbarItem[]
): SessionSummaryWorkbarItem[] => {
  if (incoming.length === 0) {
    return current;
  }

  const merged = [...current];
  const indexById = new Map<string, number>();
  merged.forEach((item, index) => {
    if (item.id) {
      indexById.set(item.id, index);
    }
  });

  for (const nextItem of incoming) {
    const summaryId = nextItem.id.trim();
    if (!summaryId) {
      merged.push(nextItem);
      continue;
    }

    const existingIndex = indexById.get(summaryId);
    if (typeof existingIndex === 'number') {
      merged[existingIndex] = nextItem;
      continue;
    }

    indexById.set(summaryId, merged.length);
    merged.push(nextItem);
  }

  return merged;
};

export const ChatInterface: React.FC<ChatInterfaceProps> = ({
  className,
  onMessageSend,
  customRenderer,
}) => {
  const {
    currentSession,
    currentProject,
    currentTerminal,
    currentRemoteConnection,
    projects,
    activePanel,
    messages,
    hasMoreMessages,
    error,
    loadSessions,
    loadProjects,
    // selectSession,
    loadMoreMessages,
    toggleTurnProcess,
    sendMessage,
    clearError,
    sidebarOpen,
    toggleSidebar,
    aiModelConfigs,
    selectedModelId,
    setSelectedModel,
    loadAiModelConfigs,
    loadAgents,
    chatConfig,
    updateChatConfig,
    abortCurrentConversation,
    sessionChatState = {},
    taskReviewPanelsBySession = {},
    uiPromptPanelsBySession = {},
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
    // applications,  // 涓嶅啀鍦ㄦ缁勪欢涓娇鐢?
    // selectedApplicationId,  // 涓嶅啀鐢ㄤ簬鑷姩鏄剧ず
  } = useChatStoreSelector((state) => ({
    currentSession: state.currentSession,
    currentProject: state.currentProject,
    currentTerminal: state.currentTerminal,
    currentRemoteConnection: state.currentRemoteConnection,
    projects: state.projects,
    activePanel: state.activePanel,
    messages: state.messages,
    hasMoreMessages: state.hasMoreMessages,
    error: state.error,
    loadSessions: state.loadSessions,
    loadProjects: state.loadProjects,
    loadMoreMessages: state.loadMoreMessages,
    toggleTurnProcess: state.toggleTurnProcess,
    sendMessage: state.sendMessage,
    clearError: state.clearError,
    sidebarOpen: state.sidebarOpen,
    toggleSidebar: state.toggleSidebar,
    aiModelConfigs: state.aiModelConfigs,
    selectedModelId: state.selectedModelId,
    setSelectedModel: state.setSelectedModel,
    loadAiModelConfigs: state.loadAiModelConfigs,
    loadAgents: state.loadAgents,
    chatConfig: state.chatConfig,
    updateChatConfig: state.updateChatConfig,
    abortCurrentConversation: state.abortCurrentConversation,
    sessionChatState: state.sessionChatState,
    taskReviewPanelsBySession: state.taskReviewPanelsBySession,
    uiPromptPanelsBySession: state.uiPromptPanelsBySession,
    upsertTaskReviewPanel: state.upsertTaskReviewPanel,
    removeTaskReviewPanel: state.removeTaskReviewPanel,
    upsertUiPromptPanel: state.upsertUiPromptPanel,
    removeUiPromptPanel: state.removeUiPromptPanel,
  }), shallow);

  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);
  const { user, logout } = useAuthStore();

  const activeModelConfig = useMemo(
    () => aiModelConfigs.find((m: any) => m.id === selectedModelId),
    [aiModelConfigs, selectedModelId]
  );
  const supportsImages = activeModelConfig?.supports_images === true;
  const supportsReasoning = activeModelConfig?.supports_reasoning === true;
  const supportedFileTypes = useMemo(() => (
    supportsImages
      ? ['image/*', 'text/*', 'application/json', 'application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']
      : ['text/*', 'application/json', 'application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']
  ), [supportsImages]);
  const currentChatState = useMemo(() => (
    currentSession ? sessionChatState[currentSession.id] : undefined
  ), [currentSession, sessionChatState]);
  const chatIsLoading = currentChatState?.isLoading ?? false;
  const chatIsStreaming = currentChatState?.isStreaming ?? false;
  const chatIsStopping = currentChatState?.isStopping ?? false;
  const headerTitle = activePanel === 'project'
    ? (currentProject?.name || '项目')
    : activePanel === 'terminal'
      ? (currentTerminal?.name || '终端')
      : activePanel === 'remote_terminal' || activePanel === 'remote_sftp'
        ? (currentRemoteConnection?.name || '远端连接')
      : (currentSession?.title || '');

  const [showMcpManager, setShowMcpManager] = useState(false);
  const [showAiModelManager, setShowAiModelManager] = useState(false);
  const [showSystemContextEditor, setShowSystemContextEditor] = useState(false);
  const [showApplicationsPanel, setShowApplicationsPanel] = useState(false);
  const [showNotepadPanel, setShowNotepadPanel] = useState(false);
  const [showUserSettings, setShowUserSettings] = useState(false);
  const didInitRef = useRef(false);
  const [workbarCurrentTurnTasks, setWorkbarCurrentTurnTasks] = useState<TaskWorkbarItem[]>([]);
  const [workbarHistoryTasks, setWorkbarHistoryTasks] = useState<TaskWorkbarItem[]>([]);
  const [workbarHistoryLoadedSessionId, setWorkbarHistoryLoadedSessionId] = useState<string | null>(null);
  const [workbarLoading, setWorkbarLoading] = useState(false);
  const [workbarHistoryLoading, setWorkbarHistoryLoading] = useState(false);
  const [workbarSummaries, setWorkbarSummaries] = useState<SessionSummaryWorkbarItem[]>([]);
  const [workbarSummariesTotal, setWorkbarSummariesTotal] = useState(0);
  const [workbarSummariesLoadedSessionId, setWorkbarSummariesLoadedSessionId] = useState<string | null>(null);
  const [workbarSummariesLoading, setWorkbarSummariesLoading] = useState(false);
  const [workbarSummariesLoadingMore, setWorkbarSummariesLoadingMore] = useState(false);
  const [workbarError, setWorkbarError] = useState<string | null>(null);
  const [workbarHistoryError, setWorkbarHistoryError] = useState<string | null>(null);
  const [workbarSummariesError, setWorkbarSummariesError] = useState<string | null>(null);
  const [summaryPaneSessionId, setSummaryPaneSessionId] = useState<string | null>(null);
  const [uiPromptHistoryOpen, setUiPromptHistoryOpen] = useState(false);
  const [uiPromptHistoryItems, setUiPromptHistoryItems] = useState<UiPromptHistoryItem[]>([]);
  const [uiPromptHistoryLoading, setUiPromptHistoryLoading] = useState(false);
  const [uiPromptHistoryError, setUiPromptHistoryError] = useState<string | null>(null);
  const [uiPromptHistoryLoadedSessionId, setUiPromptHistoryLoadedSessionId] = useState<string | null>(null);
  const [composerProjectId, setComposerProjectId] = useState<string | null>(null);
  const [composerMcpEnabled, setComposerMcpEnabled] = useState(true);
  const [composerEnabledMcpIds, setComposerEnabledMcpIds] = useState<string[]>([]);
  const currentSessionRef = useRef<string | null>(null);
  const lastHydratedChatSessionRef = useRef<string | null>(null);
  const currentTurnLoadSeqRef = useRef(0);
  const historyLoadSeqRef = useRef(0);
  const summariesLoadSeqRef = useRef(0);
  const summariesLoadMoreSeqRef = useRef(0);
  const uiPromptHistoryLoadSeqRef = useRef(0);
  const uiPromptHistoryCacheRef = useRef<Map<string, UiPromptHistoryItem[]>>(new Map());

  const activeTaskReviewPanel = useMemo(() => {
    if (!currentSession) {
      return null;
    }
    const panels = taskReviewPanelsBySession[currentSession.id];
    if (!Array.isArray(panels) || panels.length === 0) {
      return null;
    }
    return panels[0];
  }, [currentSession, taskReviewPanelsBySession]);

  const activeUiPromptPanel = useMemo(() => {
    if (!currentSession) {
      return null;
    }
    const panels = uiPromptPanelsBySession[currentSession.id];
    if (!Array.isArray(panels) || panels.length === 0) {
      return null;
    }
    return panels[0];
  }, [currentSession, uiPromptPanelsBySession]);

  const activeConversationTurnId = useMemo(() => {
    if (!currentSession) {
      return null;
    }

    for (let i = messages.length - 1; i >= 0; i -= 1) {
      const message = messages[i] as any;
      if (message?.sessionId && message.sessionId !== currentSession.id) {
        continue;
      }
      const turnId = typeof message?.metadata?.conversation_turn_id === 'string'
        ? message.metadata.conversation_turn_id.trim()
        : '';
      if (turnId) {
        return turnId;
      }
    }

    return null;
  }, [currentSession, messages]);

  const handledTaskMutationKeysRef = useRef<Set<string>>(new Set());

  const sessionSummaryPaneVisible = Boolean(
    activePanel === 'chat' && currentSession && summaryPaneSessionId === currentSession.id
  );
  const summariesHasMore = workbarSummaries.length < workbarSummariesTotal;

  const currentSessionIdForUiPrompts = currentSession?.id || null;

  useEffect(() => {
    currentSessionRef.current = currentSession?.id || null;
  }, [currentSession?.id]);

  useEffect(() => {
    const runtime = readSessionRuntimeFromMetadata(currentSession?.metadata);
    setComposerProjectId(runtime?.projectId ?? currentProject?.id ?? null);
    setComposerMcpEnabled(runtime?.mcpEnabled ?? true);
    setComposerEnabledMcpIds(runtime?.enabledMcpIds ?? []);
  }, [currentProject?.id, currentSession?.id, currentSession?.metadata]);

  useEffect(() => {
    if (!currentSessionIdForUiPrompts || activePanel !== 'chat') {
      return;
    }

    let cancelled = false;
    void apiClient
      .getPendingUiPrompts(currentSessionIdForUiPrompts, { limit: 50 })
      .then((records) => {
        if (cancelled || !Array.isArray(records)) {
          return;
        }
        records.forEach((record) => {
          const panel = toUiPromptPanelFromRecord(record);
          if (panel) {
            upsertUiPromptPanel(panel);
          }
        });
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [activePanel, apiClient, currentSessionIdForUiPrompts, upsertUiPromptPanel]);

  const loadUiPromptHistory = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId) {
      setUiPromptHistoryItems([]);
      setUiPromptHistoryError(null);
      setUiPromptHistoryLoadedSessionId(null);
      setUiPromptHistoryLoading(false);
      return;
    }

    const cached = uiPromptHistoryCacheRef.current.get(sessionId);
    if (!force && uiPromptHistoryLoadedSessionId === sessionId && uiPromptHistoryItems.length > 0) {
      return;
    }
    if (!force && cached) {
      setUiPromptHistoryItems(cached);
      setUiPromptHistoryError(null);
      setUiPromptHistoryLoadedSessionId(sessionId);
      setUiPromptHistoryLoading(false);
      return;
    }

    const requestSeq = uiPromptHistoryLoadSeqRef.current + 1;
    uiPromptHistoryLoadSeqRef.current = requestSeq;
    const shouldShowLoading = force || !cached;
    if (shouldShowLoading) {
      setUiPromptHistoryLoading(true);
    }
    setUiPromptHistoryError(null);
    try {
      const records = await apiClient.getUiPromptHistory(sessionId, { limit: 200 });
      const normalized = Array.isArray(records)
        ? records
            .map((item) => normalizeUiPromptHistoryItem(item))
            .filter((item): item is UiPromptHistoryItem => item !== null)
        : [];
      uiPromptHistoryCacheRef.current.set(sessionId, normalized);
      if (
        uiPromptHistoryLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setUiPromptHistoryItems(normalized);
      setUiPromptHistoryLoadedSessionId(sessionId);
    } catch (error) {
      if (
        uiPromptHistoryLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setUiPromptHistoryError(error instanceof Error ? error.message : '交互确认记录加载失败');
    } finally {
      if (
        uiPromptHistoryLoadSeqRef.current === requestSeq
        && currentSessionRef.current === sessionId
      ) {
        setUiPromptHistoryLoading(false);
      }
    }
  }, [apiClient, uiPromptHistoryItems.length, uiPromptHistoryLoadedSessionId]);

  const CURRENT_TURN_MUTATION_FALLBACK_LIMIT = 8;

  const currentTurnMutationTaskIds = useMemo(() => {
    if (!currentSession || !activeConversationTurnId) {
      return [];
    }

    const ids = new Set<string>();
    let lastKnownTurnId = '';

    for (const message of messages as any[]) {
      if (message?.sessionId && message.sessionId !== currentSession.id) {
        continue;
      }

      const messageTurnId = typeof message?.metadata?.conversation_turn_id === 'string'
        ? message.metadata.conversation_turn_id.trim()
        : '';
      if (messageTurnId) {
        lastKnownTurnId = messageTurnId;
      }

      const effectiveTurnId = messageTurnId || lastKnownTurnId;
      if (effectiveTurnId !== activeConversationTurnId) {
        continue;
      }

      const toolCalls = collectMessageToolCalls(message);
      for (const toolCall of toolCalls) {
        if (!shouldRefreshForTaskMutationToolCall(toolCall)) {
          continue;
        }
        if (toolCall?.completed !== true || hasToolCallError(toolCall)) {
          continue;
        }

        extractTaskIdsFromToolCall(toolCall).forEach((taskId) => ids.add(taskId));
      }
    }

    return Array.from(ids);
  }, [
    activeConversationTurnId,
    currentSession,
    messages,
  ]);

  const mergedCurrentTurnTasks = useMemo(() => {
    const baseTasks = workbarCurrentTurnTasks.length > 0
      ? workbarCurrentTurnTasks
      : selectLatestTurnTasks(workbarHistoryTasks);

    if (currentTurnMutationTaskIds.length === 0) {
      return baseTasks;
    }

    const existing = new Set(baseTasks.map((task) => task.id));
    const fallbackCandidates = workbarHistoryTasks
      .filter((task) => currentTurnMutationTaskIds.includes(task.id) && !existing.has(task.id))
      .slice(0, CURRENT_TURN_MUTATION_FALLBACK_LIMIT);

    if (fallbackCandidates.length === 0) {
      return baseTasks;
    }

    return [...baseTasks, ...fallbackCandidates];
  }, [currentTurnMutationTaskIds, selectLatestTurnTasks, workbarCurrentTurnTasks, workbarHistoryTasks]);

  const loadCurrentTurnWorkbarTasks = useCallback(async (sessionId: string, conversationTurnId?: string | null) => {
    if (!sessionId) {
      setWorkbarCurrentTurnTasks([]);
      setWorkbarError(null);
      setWorkbarLoading(false);
      return;
    }

    const requestSeq = currentTurnLoadSeqRef.current + 1;
    currentTurnLoadSeqRef.current = requestSeq;
    const turnId = typeof conversationTurnId === 'string' ? conversationTurnId.trim() : '';

    setWorkbarLoading(true);
    setWorkbarError(null);
    try {
      let normalizedTasks: TaskWorkbarItem[] = [];

      if (turnId) {
        const tasks = await apiClient.getTaskManagerTasks(sessionId, {
          conversationTurnId: turnId,
          includeDone: true,
          limit: 100,
        });
        normalizedTasks = tasks.map(normalizeWorkbarTask);
      }

      if (normalizedTasks.length === 0) {
        const fallbackTasks = await apiClient.getTaskManagerTasks(sessionId, {
          includeDone: true,
          limit: 200,
        });
        normalizedTasks = selectLatestTurnTasks(fallbackTasks.map(normalizeWorkbarTask));
      }

      if (
        currentTurnLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarCurrentTurnTasks(normalizedTasks);
    } catch (error) {
      if (
        currentTurnLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarError(error instanceof Error ? error.message : '任务操作失败');
    } finally {
      if (
        currentTurnLoadSeqRef.current === requestSeq
        && currentSessionRef.current === sessionId
      ) {
        setWorkbarLoading(false);
      }
    }
  }, [apiClient]);

  const loadHistoryWorkbarTasks = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId) {
      setWorkbarHistoryTasks([]);
      setWorkbarHistoryError(null);
      setWorkbarHistoryLoadedSessionId(null);
      setWorkbarHistoryLoading(false);
      return;
    }

    if (!force && workbarHistoryLoadedSessionId === sessionId && workbarHistoryTasks.length > 0) {
      return;
    }

    const requestSeq = historyLoadSeqRef.current + 1;
    historyLoadSeqRef.current = requestSeq;
    setWorkbarHistoryLoading(true);
    setWorkbarHistoryError(null);
    try {
      const tasks = await apiClient.getTaskManagerTasks(sessionId, {
        includeDone: true,
        limit: 300,
      });
      if (
        historyLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarHistoryTasks(tasks.map(normalizeWorkbarTask));
      setWorkbarHistoryLoadedSessionId(sessionId);
    } catch (error) {
      if (
        historyLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarHistoryError(error instanceof Error ? error.message : '\u4efb\u52a1\u52a0\u8f7d\u5931\u8d25');
    } finally {
      if (
        historyLoadSeqRef.current === requestSeq
        && currentSessionRef.current === sessionId
      ) {
        setWorkbarHistoryLoading(false);
      }
    }
  }, [apiClient, workbarHistoryLoadedSessionId, workbarHistoryTasks.length]);

  const loadWorkbarSummaries = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId) {
      setWorkbarSummaries([]);
      setWorkbarSummariesTotal(0);
      setWorkbarSummariesLoadedSessionId(null);
      setWorkbarSummariesError(null);
      setWorkbarSummariesLoading(false);
      setWorkbarSummariesLoadingMore(false);
      return;
    }

    if (!force && workbarSummariesLoadedSessionId === sessionId) {
      return;
    }

    const requestSeq = summariesLoadSeqRef.current + 1;
    summariesLoadSeqRef.current = requestSeq;
    setWorkbarSummariesLoading(true);
    setWorkbarSummariesLoadingMore(false);
    setWorkbarSummariesError(null);
    try {
      const payload = await apiClient.getSessionSummaries(sessionId, {
        limit: WORKBAR_SUMMARY_PAGE_SIZE,
        offset: 0,
      });
      const items = Array.isArray(payload?.items)
        ? payload.items.map(normalizeWorkbarSummary)
        : [];
      if (
        summariesLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarSummaries(items);
      const total = typeof payload?.total === 'number' ? payload.total : items.length;
      setWorkbarSummariesTotal(Math.max(total, items.length));
      setWorkbarSummariesLoadedSessionId(sessionId);
    } catch (error) {
      if (
        summariesLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarSummariesError(error instanceof Error ? error.message : '会话总结加载失败');
    } finally {
      if (
        summariesLoadSeqRef.current === requestSeq
        && currentSessionRef.current === sessionId
      ) {
        setWorkbarSummariesLoading(false);
      }
    }
  }, [apiClient, workbarSummariesLoadedSessionId]);

  const loadMoreWorkbarSummaries = useCallback(async (sessionId: string) => {
    if (!sessionId || workbarSummariesLoading || workbarSummariesLoadingMore) {
      return;
    }
    if (workbarSummariesLoadedSessionId !== sessionId) {
      await loadWorkbarSummaries(sessionId, true);
      return;
    }
    if (workbarSummaries.length >= workbarSummariesTotal) {
      return;
    }

    const offset = workbarSummaries.length;
    const requestSeq = summariesLoadMoreSeqRef.current + 1;
    summariesLoadMoreSeqRef.current = requestSeq;
    setWorkbarSummariesLoadingMore(true);
    setWorkbarSummariesError(null);
    try {
      const payload = await apiClient.getSessionSummaries(sessionId, {
        limit: WORKBAR_SUMMARY_PAGE_SIZE,
        offset,
      });
      const items = Array.isArray(payload?.items)
        ? payload.items.map(normalizeWorkbarSummary)
        : [];
      if (
        summariesLoadMoreSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarSummaries((previous) => appendUniqueSummaries(previous, items));
      const total = typeof payload?.total === 'number' ? payload.total : workbarSummariesTotal;
      setWorkbarSummariesTotal(Math.max(total, offset + items.length));
    } catch (error) {
      if (
        summariesLoadMoreSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarSummariesError(error instanceof Error ? error.message : '会话总结加载失败');
    } finally {
      if (
        summariesLoadMoreSeqRef.current === requestSeq
        && currentSessionRef.current === sessionId
      ) {
        setWorkbarSummariesLoadingMore(false);
      }
    }
  }, [
    apiClient,
    loadWorkbarSummaries,
    workbarSummaries.length,
    workbarSummariesLoadedSessionId,
    workbarSummariesLoading,
    workbarSummariesLoadingMore,
    workbarSummariesTotal,
  ]);

  const handleOpenSessionSummaryPane = useCallback((sessionId: string) => {
    if (!sessionId) {
      return;
    }
    setSummaryPaneSessionId(sessionId);
    void loadWorkbarSummaries(sessionId, true);
  }, [loadWorkbarSummaries]);

  const refreshWorkbarTasks = useCallback(async () => {
    if (!currentSession) {
      return;
    }
    await Promise.all([
      loadCurrentTurnWorkbarTasks(currentSession.id, activeConversationTurnId),
      loadHistoryWorkbarTasks(currentSession.id, true),
      loadWorkbarSummaries(currentSession.id, true),
    ]);
  }, [activeConversationTurnId, currentSession, loadCurrentTurnWorkbarTasks, loadHistoryWorkbarTasks, loadWorkbarSummaries]);

  useEffect(() => {
    if (!currentSession || activePanel !== 'chat') {
      return;
    }

    const handled = handledTaskMutationKeysRef.current;
    const pendingKeys: string[] = [];

    for (const message of messages as any[]) {
      if (message?.sessionId && message.sessionId !== currentSession.id) {
        continue;
      }

      const toolCalls = collectMessageToolCalls(message);
      for (const toolCall of toolCalls) {
        if (!shouldRefreshForTaskMutationToolCall(toolCall)) {
          continue;
        }
        if (toolCall?.completed !== true) {
          continue;
        }
        if (hasToolCallError(toolCall)) {
          continue;
        }

        const toolCallId = String(toolCall?.id || toolCall?.tool_call_id || toolCall?.toolCallId || '').trim();
        const key = currentSession.id + ':' + String(message?.id || '') + ':' + (toolCallId || String(toolCall?.name || ''));
        if (handled.has(key)) {
          continue;
        }
        pendingKeys.push(key);
      }
    }

    if (pendingKeys.length === 0) {
      return;
    }

    pendingKeys.forEach((key) => handled.add(key));
    if (handled.size > 2048) {
      const tail = Array.from(handled).slice(-1024);
      handled.clear();
      tail.forEach((key) => handled.add(key));
    }
    void loadCurrentTurnWorkbarTasks(currentSession.id, activeConversationTurnId);
    if (sessionSummaryPaneVisible) {
      void loadWorkbarSummaries(currentSession.id, true);
    }
  }, [
    activeConversationTurnId,
    activePanel,
    currentSession,
    loadCurrentTurnWorkbarTasks,
    loadWorkbarSummaries,
    messages,
    sessionSummaryPaneVisible,
  ]);

  const {
    workbarActionLoadingTaskId,
    workbarSummaryActionLoadingKey,
    handleWorkbarCompleteTask,
    handleWorkbarDeleteTask,
    handleWorkbarEditTask,
    handleDeleteWorkbarSummary,
    handleClearWorkbarSummaries,
  } = useWorkbarMutations({
    apiClient,
    currentSessionId: currentSession?.id ?? null,
    workbarSummariesLength: workbarSummaries.length,
    refreshWorkbarTasks,
    loadWorkbarSummaries,
    setWorkbarError,
    setWorkbarSummariesError,
  });

  // 鍒濆鍖栧姞杞戒細璇濄€丄I妯″瀷鍜屾櫤鑳戒綋閰嶇疆
  useEffect(() => {
    // React 18 鍦ㄥ紑鍙戞ā寮忎笅浼氬弻璋冪敤鍓綔鐢紝杩欓噷鍔犱竴娆℃€т繚鎶わ紙缁勪欢鍐咃級
    if (didInitRef.current) return;
    didInitRef.current = true;

    loadSessions({ limit: SESSION_PAGE_SIZE, offset: 0 });
    loadProjects();
    loadAiModelConfigs();
    loadAgents();
  }, [loadSessions, loadProjects, loadAiModelConfigs, loadAgents]);

  useEffect(() => {
    if (!currentSession || activePanel !== 'chat') {
      currentTurnLoadSeqRef.current += 1;
      historyLoadSeqRef.current += 1;
      summariesLoadSeqRef.current += 1;
      summariesLoadMoreSeqRef.current += 1;
      uiPromptHistoryLoadSeqRef.current += 1;
      lastHydratedChatSessionRef.current = null;
      setWorkbarCurrentTurnTasks([]);
      setWorkbarHistoryTasks([]);
      setWorkbarSummaries([]);
      setWorkbarSummariesTotal(0);
      setWorkbarError(null);
      setWorkbarHistoryError(null);
      setWorkbarSummariesError(null);
      setWorkbarLoading(false);
      setWorkbarHistoryLoading(false);
      setWorkbarSummariesLoading(false);
      setWorkbarHistoryLoadedSessionId(null);
      setWorkbarSummariesLoadedSessionId(null);
      setWorkbarSummariesLoadingMore(false);
      setUiPromptHistoryItems([]);
      setUiPromptHistoryError(null);
      setUiPromptHistoryLoadedSessionId(null);
      setUiPromptHistoryLoading(false);
      setUiPromptHistoryOpen(false);
      return;
    }

    const sessionChanged = lastHydratedChatSessionRef.current !== currentSession.id;
    if (sessionChanged) {
      lastHydratedChatSessionRef.current = currentSession.id;
      historyLoadSeqRef.current += 1;
      summariesLoadSeqRef.current += 1;
      summariesLoadMoreSeqRef.current += 1;
      uiPromptHistoryLoadSeqRef.current += 1;
      setWorkbarHistoryTasks([]);
      setWorkbarHistoryError(null);
      setWorkbarHistoryLoadedSessionId(null);
      setWorkbarHistoryLoading(false);
      setWorkbarSummaries([]);
      setWorkbarSummariesTotal(0);
      setWorkbarSummariesError(null);
      setWorkbarSummariesLoadedSessionId(null);
      setWorkbarSummariesLoading(false);
      setWorkbarSummariesLoadingMore(false);
      const cachedUiPromptHistory = uiPromptHistoryCacheRef.current.get(currentSession.id);
      setUiPromptHistoryItems(cachedUiPromptHistory ? [...cachedUiPromptHistory] : []);
      setUiPromptHistoryError(null);
      setUiPromptHistoryLoadedSessionId(cachedUiPromptHistory ? currentSession.id : null);
      setUiPromptHistoryLoading(false);
    }

    void loadCurrentTurnWorkbarTasks(currentSession.id, activeConversationTurnId);
    if (sessionSummaryPaneVisible) {
      void loadWorkbarSummaries(currentSession.id);
    }
    if (uiPromptHistoryOpen) {
      void loadUiPromptHistory(currentSession.id);
    }
  }, [
    activeConversationTurnId,
    activePanel,
    currentSession,
    loadCurrentTurnWorkbarTasks,
    loadWorkbarSummaries,
    loadUiPromptHistory,
    sessionSummaryPaneVisible,
    uiPromptHistoryOpen,
  ]);

  // 澶勭悊娑堟伅鍙戦€?
  const handleMessageSend = useCallback(async (
    content: string,
    attachments?: File[],
    runtimeOptions?: {
      mcpEnabled?: boolean;
      projectId?: string | null;
      projectRoot?: string | null;
      enabledMcpIds?: string[];
    },
  ) => {
    try {
      await sendMessage(content, attachments, runtimeOptions);
      onMessageSend?.(content, attachments);
    } catch (error) {
      console.error('Failed to send message:', error);
    }
  }, [onMessageSend, sendMessage]);

  const handleLoadMore = useCallback(() => {
    if (currentSession) {
      loadMoreMessages(currentSession.id);
    }
  }, [currentSession, loadMoreMessages]);

  const handleToggleTurnProcess = useCallback((userMessageId: string) => {
    if (!userMessageId) {
      return;
    }
    void toggleTurnProcess(userMessageId)
      .catch((error) => {
        console.error('Failed to toggle turn process messages:', error);
      });
  }, [toggleTurnProcess]);

  const {
    handleTaskReviewConfirm,
    handleTaskReviewCancel,
    handleUiPromptSubmit,
    handleUiPromptCancel,
  } = usePanelActions({
    activeTaskReviewPanel,
    activeUiPromptPanel,
    apiClient,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    loadWorkbarSummaries,
    loadUiPromptHistory,
  });


  if (showSystemContextEditor) {
    return (
      <SystemContextEditor onClose={() => setShowSystemContextEditor(false)} />
    );
  }

  return (
    <div className={cn(
      'flex flex-col h-screen bg-background text-foreground',
      className
    )}>
      <HeaderBar
        headerTitle={headerTitle}
        sidebarOpen={sidebarOpen}
        onToggleSidebar={toggleSidebar}
        onOpenNotepad={() => setShowNotepadPanel(true)}
        onOpenApplications={() => setShowApplicationsPanel(true)}
        onOpenMcpManager={() => setShowMcpManager(true)}
        onOpenAiModelManager={() => setShowAiModelManager(true)}
        onOpenSystemContextEditor={() => setShowSystemContextEditor(true)}
        onOpenUserSettings={() => setShowUserSettings(true)}
        onLogout={logout}
        user={user}
      />

          {/* 閿欒鎻愮ず */}
          {error && (
            <div className="mx-4 mt-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg">
              <div className="flex items-center justify-between">
                <p className="text-sm text-destructive">{error}</p>
                <button
                  onClick={clearError}
                  className="text-destructive hover:text-destructive/80 transition-colors"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </div>
          )}

        {/* 涓诲尯鍩燂細宸︿晶浼氳瘽鍒楄〃 + 鍙充晶鑱婂ぉ */}
        <div className="flex flex-1 min-h-0 overflow-hidden">
          <SessionList
            collapsed={!sidebarOpen}
            onToggleCollapse={toggleSidebar}
            onSelectSession={() => setSummaryPaneSessionId(null)}
            onOpenSummary={handleOpenSessionSummaryPane}
            summaryOpenSessionId={sessionSummaryPaneVisible ? currentSession?.id ?? null : null}
          />

          {/* 宸茬Щ闄ゅ乏渚у簲鐢ㄦ娊灞夐潰鏉匡紝鏀逛负寮圭獥 */}
          {/* 宓屽叆鍖哄煙宸茬Щ闄?- 搴旂敤閫夋嫨鍚庡彧瑙﹀彂浜嬩欢锛屼笉鑷姩鏄剧ず */}
          {/* 澶栭儴鍙互閫氳繃 subscribeSelectedApplication 鐩戝惉搴旂敤閫夋嫨浜嬩欢 */}
          {/* 鐒跺悗鑷鍐冲畾濡備綍鎵撳紑/鏄剧ず搴旂敤锛圗lectron 绐楀彛銆亀indow.open 绛夛級 */}

          <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
            {activePanel === 'project' ? (
              <ProjectExplorer project={currentProject} className="flex-1" />
            ) : activePanel === 'terminal' ? (
              <TerminalView className="flex-1" />
            ) : activePanel === 'remote_terminal' ? (
              <RemoteTerminalView className="flex-1" />
            ) : activePanel === 'remote_sftp' ? (
              <RemoteSftpPanel className="flex-1" />
            ) : (
              <div className="flex-1 min-h-0 flex overflow-hidden">
                <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
                  <div className="flex-1 overflow-hidden">
                    {currentSession ? (
                      sessionSummaryPaneVisible ? (
                        <SummaryPane
                          sessionId={currentSession.id}
                          sessionTitle={currentSession.title}
                          messages={messages}
                          isLoading={chatIsLoading}
                          isStreaming={chatIsStreaming}
                          isStopping={chatIsStopping}
                          hasMore={hasMoreMessages}
                          onLoadMore={handleLoadMore}
                          onToggleTurnProcess={handleToggleTurnProcess}
                          customRenderer={customRenderer}
                          summaries={workbarSummaries}
                          summariesLoading={workbarSummariesLoading}
                          summariesLoadingMore={workbarSummariesLoadingMore}
                          summariesHasMore={summariesHasMore}
                          summariesError={workbarSummariesError}
                          actionLoadingKey={workbarSummaryActionLoadingKey}
                          onClearAll={() => {
                            void handleClearWorkbarSummaries();
                          }}
                          onRefresh={() => {
                            void loadWorkbarSummaries(currentSession.id, true);
                          }}
                          onClose={() => setSummaryPaneSessionId(null)}
                          onDeleteSummary={(summary) => {
                            void handleDeleteWorkbarSummary(summary);
                          }}
                          onLoadMoreSummaries={() => {
                            void loadMoreWorkbarSummaries(currentSession.id);
                          }}
                          formatCreatedAt={formatSummaryCreatedAt}
                        />
                      ) : (
                        <MessageList
                          key={`messages-${currentSession?.id || 'none'}-chat`}
                          sessionId={currentSession?.id}
                          messages={messages}
                          isLoading={chatIsLoading}
                          isStreaming={chatIsStreaming}
                          isStopping={chatIsStopping}
                          hasMore={hasMoreMessages}
                          onLoadMore={handleLoadMore}
                          onToggleTurnProcess={handleToggleTurnProcess}
                          customRenderer={customRenderer}
                        />
                      )
                    ) : (
                      <div className="flex items-center justify-center h-full">
                        <div className="text-center">
                          <h2 className="text-xl font-semibold text-muted-foreground mb-2">
                            欢迎使用 AI 聊天
                          </h2>
                          <p className="text-muted-foreground mb-4">
                            点击左上角按钮选择会话，或创建新的会话开始对话
                          </p>
                          <button
                            onClick={toggleSidebar}
                            className="px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
                          >
                            展开会话列表
                          </button>
                        </div>
                      </div>
                    )}
                  </div>

                  {/* 杈撳叆鍖哄煙 */}
                  {currentSession && activePanel === 'chat' && (
                    <ChatComposerPanel
                      sessionId={currentSession.id}
                      mergedCurrentTurnTasks={mergedCurrentTurnTasks}
                      workbarHistoryTasks={workbarHistoryTasks}
                      activeConversationTurnId={activeConversationTurnId}
                      workbarLoading={workbarLoading}
                      workbarHistoryLoading={workbarHistoryLoading}
                      workbarError={workbarError}
                      workbarHistoryError={workbarHistoryError}
                      workbarActionLoadingTaskId={workbarActionLoadingTaskId}
                      onRefreshWorkbarTasks={() => {
                        void refreshWorkbarTasks();
                      }}
                      onOpenHistory={(sessionId) => {
                        void loadHistoryWorkbarTasks(sessionId);
                        void loadWorkbarSummaries(sessionId);
                      }}
                      onOpenUiPromptHistory={(sessionId) => {
                        setUiPromptHistoryOpen(true);
                        void loadUiPromptHistory(sessionId);
                      }}
                      uiPromptHistoryCount={uiPromptHistoryItems.length}
                      uiPromptHistoryLoading={uiPromptHistoryLoading}
                      onCompleteTask={(task) => {
                        void handleWorkbarCompleteTask(task);
                      }}
                      onDeleteTask={(task) => {
                        void handleWorkbarDeleteTask(task);
                      }}
                      onEditTask={(task) => {
                        void handleWorkbarEditTask(task);
                      }}
                      activeUiPromptPanel={activeUiPromptPanel}
                      onUiPromptSubmit={handleUiPromptSubmit}
                      onUiPromptCancel={handleUiPromptCancel}
                      activeTaskReviewPanel={activeTaskReviewPanel}
                      onTaskReviewConfirm={handleTaskReviewConfirm}
                      onTaskReviewCancel={handleTaskReviewCancel}
                      onSend={handleMessageSend}
                      onStop={abortCurrentConversation}
                      inputDisabled={chatIsLoading || chatIsStreaming || chatIsStopping}
                      isStreaming={chatIsStreaming}
                      isStopping={chatIsStopping}
                      supportedFileTypes={supportedFileTypes}
                      reasoningSupported={supportsReasoning}
                      reasoningEnabled={chatConfig?.reasoningEnabled === true}
                      onReasoningToggle={(enabled) => updateChatConfig({ reasoningEnabled: enabled })}
                      selectedModelId={selectedModelId}
                      availableModels={aiModelConfigs}
                      onModelChange={setSelectedModel}
                      availableProjects={projects}
                      currentProject={currentProject}
                      selectedProjectId={composerProjectId}
                      onProjectChange={setComposerProjectId}
                      mcpEnabled={composerMcpEnabled}
                      enabledMcpIds={composerEnabledMcpIds}
                      onMcpEnabledChange={setComposerMcpEnabled}
                    />
                  )}
                </div>
              </div>
            )}
          </div>

        </div>
        
        <UiPromptHistoryDrawer
          open={uiPromptHistoryOpen}
          items={uiPromptHistoryItems}
          loading={uiPromptHistoryLoading}
          error={uiPromptHistoryError}
          refreshDisabled={!currentSession || uiPromptHistoryLoading}
          onRefresh={() => {
            if (!currentSession) {
              return;
            }
            void loadUiPromptHistory(currentSession.id, true);
          }}
          onClose={() => setUiPromptHistoryOpen(false)}
          formatCreatedAt={formatSummaryCreatedAt}
        />

        {/* MCP绠＄悊鍣?*/}
        {showMcpManager && (
          <McpManager onClose={() => setShowMcpManager(false)} />
        )}

        <NotepadPanel
          isOpen={showNotepadPanel}
          onClose={() => setShowNotepadPanel(false)}
        />

        {/* AI妯″瀷绠＄悊鍣?*/}
        {showAiModelManager && (
          <AiModelManager onClose={() => setShowAiModelManager(false)} />
        )}
        
        {/* 绯荤粺涓婁笅鏂囩紪杈戝櫒 */}

        {showUserSettings && (
          <UserSettingsPanel onClose={() => setShowUserSettings(false)} />
        )}

        {/* 搴旂敤鍒楄〃锛堝脊绐楋級 */}
        <ApplicationsPanel
          isOpen={showApplicationsPanel}
          onClose={() => setShowApplicationsPanel(false)}
          title="应用列表"
          layout="modal"
        />

        {/* 琛ㄦ儏鍔╂墜宸茬Щ闄?*/}
    </div>
  );
};

export default ChatInterface;
