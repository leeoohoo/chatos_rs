import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useChatApiClientFromContext, useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { MessageList } from './MessageList';
import { SessionList } from './SessionList';
import McpManager from './McpManager';
import AiModelManager from './AiModelManager';
import SystemContextEditor from './SystemContextEditor';
import AgentManager from './AgentManager';
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
import type { UiPromptHistoryItem } from './chatInterface/types';
import type { SessionSummaryWorkbarItem, TaskWorkbarItem } from './TaskWorkbar';
import { apiClient as globalApiClient } from '../lib/api/client';
import { cn } from '../lib/utils';
import type { ChatInterfaceProps } from '../types';
import { useAuthStore } from '../lib/auth/authStore';

const SESSION_PAGE_SIZE = 30;

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
    agents,
    selectedAgentId,
    setSelectedAgent,
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
  } = useChatStoreFromContext();

  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);
  const { user, logout } = useAuthStore();

  const selectedAgent = useMemo(
    () => (selectedAgentId ? agents.find((a: any) => a.id === selectedAgentId) : null),
    [agents, selectedAgentId]
  );
  const activeModelConfig = useMemo(() => (
    selectedAgent
      ? aiModelConfigs.find((m: any) => m.id === selectedAgent.ai_model_config_id)
      : aiModelConfigs.find((m: any) => m.id === selectedModelId)
  ), [aiModelConfigs, selectedAgent, selectedModelId]);
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
  const [showAgentManager, setShowAgentManager] = useState(false);
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
  const [workbarSummariesLoadedSessionId, setWorkbarSummariesLoadedSessionId] = useState<string | null>(null);
  const [workbarSummariesLoading, setWorkbarSummariesLoading] = useState(false);
  const [workbarError, setWorkbarError] = useState<string | null>(null);
  const [workbarHistoryError, setWorkbarHistoryError] = useState<string | null>(null);
  const [workbarSummariesError, setWorkbarSummariesError] = useState<string | null>(null);
  const [summaryPaneSessionId, setSummaryPaneSessionId] = useState<string | null>(null);
  const [uiPromptHistoryOpen, setUiPromptHistoryOpen] = useState(false);
  const [uiPromptHistoryItems, setUiPromptHistoryItems] = useState<UiPromptHistoryItem[]>([]);
  const [uiPromptHistoryLoading, setUiPromptHistoryLoading] = useState(false);
  const [uiPromptHistoryError, setUiPromptHistoryError] = useState<string | null>(null);
  const [uiPromptHistoryLoadedSessionId, setUiPromptHistoryLoadedSessionId] = useState<string | null>(null);

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

  const currentSessionIdForUiPrompts = currentSession?.id || null;

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
      .catch((error) => {
        console.warn('Failed to load pending ui prompts:', error);
      });

    return () => {
      cancelled = true;
    };
  }, [activePanel, apiClient, currentSessionIdForUiPrompts, upsertUiPromptPanel]);

  const loadUiPromptHistory = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId) {
      setUiPromptHistoryItems([]);
      setUiPromptHistoryError(null);
      setUiPromptHistoryLoadedSessionId(null);
      return;
    }

    if (!force && uiPromptHistoryLoadedSessionId === sessionId && uiPromptHistoryItems.length > 0) {
      return;
    }

    setUiPromptHistoryLoading(true);
    setUiPromptHistoryError(null);
    try {
      const records = await apiClient.getUiPromptHistory(sessionId, { limit: 200 });
      const normalized = Array.isArray(records)
        ? records
            .map((item) => normalizeUiPromptHistoryItem(item))
            .filter((item): item is UiPromptHistoryItem => item !== null)
        : [];
      setUiPromptHistoryItems(normalized);
      setUiPromptHistoryLoadedSessionId(sessionId);
    } catch (error) {
      setUiPromptHistoryError(error instanceof Error ? error.message : '交互确认记录加载失败');
    } finally {
      setUiPromptHistoryLoading(false);
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
      return;
    }

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

      setWorkbarCurrentTurnTasks(normalizedTasks);
    } catch (error) {
      setWorkbarError(error instanceof Error ? error.message : '任务操作失败');
    } finally {
      setWorkbarLoading(false);
    }
  }, [apiClient]);

  const loadHistoryWorkbarTasks = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId) {
      setWorkbarHistoryTasks([]);
      setWorkbarHistoryError(null);
      setWorkbarHistoryLoadedSessionId(null);
      return;
    }

    if (!force && workbarHistoryLoadedSessionId === sessionId && workbarHistoryTasks.length > 0) {
      return;
    }

    setWorkbarHistoryLoading(true);
    setWorkbarHistoryError(null);
    try {
      const tasks = await apiClient.getTaskManagerTasks(sessionId, {
        includeDone: true,
        limit: 300,
      });
      setWorkbarHistoryTasks(tasks.map(normalizeWorkbarTask));
      setWorkbarHistoryLoadedSessionId(sessionId);
    } catch (error) {
      setWorkbarHistoryError(error instanceof Error ? error.message : '\u4efb\u52a1\u52a0\u8f7d\u5931\u8d25');
    } finally {
      setWorkbarHistoryLoading(false);
    }
  }, [apiClient, workbarHistoryLoadedSessionId, workbarHistoryTasks.length]);

  const loadWorkbarSummaries = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId) {
      setWorkbarSummaries([]);
      setWorkbarSummariesLoadedSessionId(null);
      setWorkbarSummariesError(null);
      return;
    }

    if (!force && workbarSummariesLoadedSessionId === sessionId) {
      return;
    }

    setWorkbarSummariesLoading(true);
    setWorkbarSummariesError(null);
    try {
      const payload = await apiClient.getSessionSummaries(sessionId, { limit: 50, offset: 0 });
      const items = Array.isArray(payload?.items)
        ? payload.items.map(normalizeWorkbarSummary)
        : [];
      setWorkbarSummaries(items);
      setWorkbarSummariesLoadedSessionId(sessionId);
    } catch (error) {
      setWorkbarSummariesError(error instanceof Error ? error.message : '会话总结加载失败');
    } finally {
      setWorkbarSummariesLoading(false);
    }
  }, [apiClient, workbarSummariesLoadedSessionId]);

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
    handledTaskMutationKeysRef.current.clear();
  }, [currentSession?.id]);

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
    void refreshWorkbarTasks();
  }, [
    activePanel,
    currentSession,
    messages,
    refreshWorkbarTasks,
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
      setWorkbarCurrentTurnTasks([]);
      setWorkbarHistoryTasks([]);
      setWorkbarSummaries([]);
      setWorkbarError(null);
      setWorkbarHistoryError(null);
      setWorkbarSummariesError(null);
      setWorkbarHistoryLoadedSessionId(null);
      setWorkbarSummariesLoadedSessionId(null);
      setUiPromptHistoryItems([]);
      setUiPromptHistoryError(null);
      setUiPromptHistoryLoadedSessionId(null);
      setUiPromptHistoryOpen(false);
      return;
    }

    void loadCurrentTurnWorkbarTasks(currentSession.id, activeConversationTurnId);
    void loadHistoryWorkbarTasks(currentSession.id);
    void loadWorkbarSummaries(currentSession.id);
    void loadUiPromptHistory(currentSession.id);
  }, [
    activeConversationTurnId,
    activePanel,
    currentSession,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    loadWorkbarSummaries,
    loadUiPromptHistory,
  ]);

  // 澶勭悊娑堟伅鍙戦€?
  const handleMessageSend = useCallback(async (content: string, attachments?: File[]) => {
    try {
      await sendMessage(content, attachments);
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
        onOpenAgentManager={() => setShowAgentManager(true)}
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
                          hasMore={hasMoreMessages}
                          onLoadMore={handleLoadMore}
                          onToggleTurnProcess={handleToggleTurnProcess}
                          customRenderer={customRenderer}
                          summaries={workbarSummaries}
                          summariesLoading={workbarSummariesLoading}
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
                          formatCreatedAt={formatSummaryCreatedAt}
                        />
                      ) : (
                        <MessageList
                          key={`messages-${currentSession?.id || 'none'}-chat`}
                          sessionId={currentSession?.id}
                          messages={messages}
                          isLoading={chatIsLoading}
                          isStreaming={chatIsStreaming}
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
                        void loadUiPromptHistory(sessionId, true);
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
                      inputDisabled={chatIsLoading || chatIsStreaming}
                      isStreaming={chatIsStreaming}
                      supportedFileTypes={supportedFileTypes}
                      reasoningSupported={supportsReasoning}
                      reasoningEnabled={chatConfig?.reasoningEnabled === true}
                      onReasoningToggle={(enabled) => updateChatConfig({ reasoningEnabled: enabled })}
                      selectedModelId={selectedModelId}
                      availableModels={aiModelConfigs}
                      onModelChange={setSelectedModel}
                      selectedAgentId={selectedAgentId}
                      availableAgents={agents}
                      onAgentChange={setSelectedAgent}
                      availableProjects={projects}
                      currentProject={currentProject}
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

        {/* 鏅鸿兘浣撶鐞嗗櫒 */}
        {showAgentManager && (
          <AgentManager onClose={() => setShowAgentManager(false)} />
        )}
        
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
