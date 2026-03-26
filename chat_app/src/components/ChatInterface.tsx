import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { shallow } from 'zustand/shallow';
import { useChatApiClientFromContext, useChatStoreSelector } from '../lib/store/ChatStoreContext';
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
import ChatConversationPane from './chatInterface/ChatConversationPane';
import HeaderBar from './chatInterface/HeaderBar';
import UiPromptHistoryDrawer from './chatInterface/UiPromptHistoryDrawer';
import TurnRuntimeContextDrawer from './chatInterface/TurnRuntimeContextDrawer';
import {
  formatSummaryCreatedAt,
  toUiPromptPanelFromRecord,
} from './chatInterface/helpers';
import { usePanelActions } from './chatInterface/usePanelActions';
import { useSessionHeaderMeta } from './chatInterface/useSessionHeaderMeta';
import { useWorkbarState } from './chatInterface/useWorkbarState';
import { useWorkbarMutations } from './chatInterface/useWorkbarMutations';
import { apiClient as globalApiClient } from '../lib/api/client';
import { cn } from '../lib/utils';
import type { ChatInterfaceProps } from '../types';
import { useAuthStore } from '../lib/auth/authStore';
import { useSessionRuntimeSettings } from '../features/sessionRuntime/useSessionRuntimeSettings';
import { useContactMemoryContext } from './chatInterface/useContactMemoryContext';
import { useUiPromptHistory } from './chatInterface/useUiPromptHistory';
import { useContactProjectScope } from './chatInterface/useContactProjectScope';
import type { TurnRuntimeSnapshotLookupResponse } from '../lib/api/client/types';

export const ChatInterface: React.FC<ChatInterfaceProps> = ({
  className,
  onMessageSend,
  customRenderer,
}) => {
  const {
    currentSession,
    contacts,
    currentProject,
    currentTerminal,
    currentRemoteConnection,
    projects,
    activePanel,
    messages,
    hasMoreMessages,
    error,
    loadProjects,
    loadMoreMessages,
    toggleTurnProcess,
    sendMessage,
    updateSession,
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
    sessionRuntimeGuidanceState = {},
    taskReviewPanelsBySession = {},
    uiPromptPanelsBySession = {},
    submitRuntimeGuidance,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
    // applications,  // 涓嶅啀鍦ㄦ缁勪欢涓娇鐢?
    // selectedApplicationId,  // 涓嶅啀鐢ㄤ簬鑷姩鏄剧ず
  } = useChatStoreSelector((state) => ({
    currentSession: state.currentSession,
    contacts: state.contacts,
    currentProject: state.currentProject,
    currentTerminal: state.currentTerminal,
    currentRemoteConnection: state.currentRemoteConnection,
    projects: state.projects,
    activePanel: state.activePanel,
    messages: state.messages,
    hasMoreMessages: state.hasMoreMessages,
    error: state.error,
    loadProjects: state.loadProjects,
    loadMoreMessages: state.loadMoreMessages,
    toggleTurnProcess: state.toggleTurnProcess,
    sendMessage: state.sendMessage,
    updateSession: state.updateSession,
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
    sessionRuntimeGuidanceState: state.sessionRuntimeGuidanceState,
    taskReviewPanelsBySession: state.taskReviewPanelsBySession,
    uiPromptPanelsBySession: state.uiPromptPanelsBySession,
    submitRuntimeGuidance: state.submitRuntimeGuidance,
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
  const {
    currentContactName,
    currentContactId,
    headerTitle,
  } = useSessionHeaderMeta({
    currentSession,
    contacts: contacts as any[],
    activePanel,
    currentProject,
    currentTerminal,
    currentRemoteConnection,
  });
  const chatIsLoading = currentChatState?.isLoading ?? false;
  const chatIsStreaming = currentChatState?.isStreaming ?? false;
  const chatIsStopping = currentChatState?.isStopping ?? false;
  const currentRuntimeGuidanceState = useMemo(() => (
    currentSession ? sessionRuntimeGuidanceState[currentSession.id] : undefined
  ), [currentSession, sessionRuntimeGuidanceState]);

  const [showMcpManager, setShowMcpManager] = useState(false);
  const [showAiModelManager, setShowAiModelManager] = useState(false);
  const [showSystemContextEditor, setShowSystemContextEditor] = useState(false);
  const [showApplicationsPanel, setShowApplicationsPanel] = useState(false);
  const [showNotepadPanel, setShowNotepadPanel] = useState(false);
  const [showUserSettings, setShowUserSettings] = useState(false);
  const didInitRef = useRef(false);
  const [summaryPaneSessionId, setSummaryPaneSessionId] = useState<string | null>(null);
  const [uiPromptHistoryOpen, setUiPromptHistoryOpen] = useState(false);
  const [runtimeContextOpen, setRuntimeContextOpen] = useState(false);
  const [runtimeContextSessionId, setRuntimeContextSessionId] = useState<string | null>(null);
  const [runtimeContextData, setRuntimeContextData] =
    useState<TurnRuntimeSnapshotLookupResponse | null>(null);
  const [runtimeContextLoading, setRuntimeContextLoading] = useState(false);
  const [runtimeContextError, setRuntimeContextError] = useState<string | null>(null);
  const {
    workspaceRoot: composerWorkspaceRoot,
    mcpEnabled: composerMcpEnabled,
    enabledMcpIds: composerEnabledMcpIds,
    setWorkspaceRoot: handleComposerWorkspaceRootChange,
    setMcpEnabled: handleComposerMcpEnabledChange,
    setEnabledMcpIds: handleComposerEnabledMcpIdsChange,
  } = useSessionRuntimeSettings({
    session: currentSession,
    updateSession,
  });
  const {
    currentProjectIdForMemory,
    currentProjectNameForMemory,
    composerAvailableProjects,
    handleComposerProjectChange,
  } = useContactProjectScope({
    apiClient,
    currentSession: currentSession as any,
    currentContactId,
    projects: projects as any[],
  });
  const {
    sessionMemorySummaries,
    agentRecalls,
    memoryLoading,
    memoryError,
    loadContactMemoryContext,
    resetMemoryState,
    cancelPendingMemoryLoad,
  } = useContactMemoryContext({
    apiClient,
    currentSessionId: currentSession?.id || null,
    currentContactId,
    currentProjectIdForMemory,
  });
  const lastHydratedChatSessionRef = useRef<string | null>(null);

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

  const sessionSummaryPaneVisible = Boolean(
    activePanel === 'chat' && currentSession && summaryPaneSessionId === currentSession.id
  );

  const currentSessionIdForUiPrompts = currentSession?.id || null;
  const {
    uiPromptHistoryItems,
    uiPromptHistoryLoading,
    uiPromptHistoryError,
    loadUiPromptHistory,
    resetUiPromptHistoryState,
    hydrateUiPromptHistoryFromCache,
    cancelPendingUiPromptHistoryLoad,
  } = useUiPromptHistory({
    apiClient,
    currentSessionId: currentSessionIdForUiPrompts,
  });

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
  const {
    activeConversationTurnId,
    mergedCurrentTurnTasks,
    workbarHistoryTasks,
    workbarLoading,
    workbarHistoryLoading,
    workbarError,
    workbarHistoryError,
    setWorkbarError,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    refreshWorkbarTasks,
    resetAllWorkbarState,
    resetHistoryWorkbarState,
  } = useWorkbarState({
    apiClient,
    currentSession,
    messages: messages as any[],
  });

  const {
    workbarActionLoadingTaskId,
    handleWorkbarCompleteTask,
    handleWorkbarDeleteTask,
    handleWorkbarEditTask,
  } = useWorkbarMutations({
    apiClient,
    currentSessionId: currentSession?.id ?? null,
    refreshWorkbarTasks,
    setWorkbarError,
  });

  // 鍒濆鍖栧姞杞戒細璇濄€丄I妯″瀷鍜屾櫤鑳戒綋閰嶇疆
  useEffect(() => {
    // React 18 鍦ㄥ紑鍙戞ā寮忎笅浼氬弻璋冪敤鍓綔鐢紝杩欓噷鍔犱竴娆℃€т繚鎶わ紙缁勪欢鍐咃級
    if (didInitRef.current) return;
    didInitRef.current = true;

    loadProjects();
    loadAiModelConfigs();
    loadAgents();
  }, [loadProjects, loadAiModelConfigs, loadAgents]);

  useEffect(() => {
    if (!currentSession || activePanel !== 'chat') {
      cancelPendingMemoryLoad();
      cancelPendingUiPromptHistoryLoad();
      lastHydratedChatSessionRef.current = null;
      resetAllWorkbarState();
      resetMemoryState();
      resetUiPromptHistoryState();
      setUiPromptHistoryOpen(false);
      return;
    }

    const sessionChanged = lastHydratedChatSessionRef.current !== currentSession.id;
    if (sessionChanged) {
      lastHydratedChatSessionRef.current = currentSession.id;
      cancelPendingMemoryLoad();
      cancelPendingUiPromptHistoryLoad();
      resetHistoryWorkbarState();
      resetMemoryState();
      hydrateUiPromptHistoryFromCache(currentSession.id);
    }

    if (sessionSummaryPaneVisible) {
      void loadContactMemoryContext(currentSession.id);
    }
    if (uiPromptHistoryOpen) {
      void loadUiPromptHistory(currentSession.id);
    }
  }, [
    activePanel,
    cancelPendingMemoryLoad,
    cancelPendingUiPromptHistoryLoad,
    currentSession,
    hydrateUiPromptHistoryFromCache,
    loadContactMemoryContext,
    loadUiPromptHistory,
    resetAllWorkbarState,
    resetHistoryWorkbarState,
    resetMemoryState,
    resetUiPromptHistoryState,
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
      workspaceRoot?: string | null;
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

  const handleRuntimeGuidanceSend = useCallback(async (content: string) => {
    const sessionId = currentSession?.id;
    const projectId = (currentSession as any)?.projectId || (currentSession as any)?.project_id || null;
    const turnId = (
      currentChatState?.activeTurnId
      || activeConversationTurnId
      || ''
    ).trim();
    if (!sessionId || !turnId) {
      return;
    }
    try {
      await submitRuntimeGuidance(content, { sessionId, turnId, projectId });
    } catch (error) {
      console.error('Failed to submit runtime guidance:', error);
    }
  }, [
    activeConversationTurnId,
    currentChatState?.activeTurnId,
    currentSession,
    currentSession?.id,
    submitRuntimeGuidance,
  ]);

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

  const handleRefreshMemory = useCallback((sessionId: string) => {
    void loadContactMemoryContext(sessionId, true);
  }, [loadContactMemoryContext]);

  const handleCloseSummary = useCallback(() => {
    setSummaryPaneSessionId(null);
  }, []);

  const handleRefreshWorkbar = useCallback(() => {
    void refreshWorkbarTasks();
  }, [refreshWorkbarTasks]);

  const handleOpenHistory = useCallback((sessionId: string) => {
    setSummaryPaneSessionId(sessionId);
    void loadHistoryWorkbarTasks(sessionId);
    void loadContactMemoryContext(sessionId, true);
  }, [loadContactMemoryContext, loadHistoryWorkbarTasks]);

  const handleOpenUiPromptHistory = useCallback((sessionId: string) => {
    setUiPromptHistoryOpen(true);
    void loadUiPromptHistory(sessionId);
  }, [loadUiPromptHistory]);

  const loadLatestRuntimeContext = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    setRuntimeContextLoading(true);
    setRuntimeContextError(null);
    try {
      const payload = await apiClient.getSessionLatestTurnRuntimeContext(sessionId);
      setRuntimeContextData(payload);
    } catch (error) {
      console.error('Failed to load turn runtime context:', error);
      setRuntimeContextError(error instanceof Error ? error.message : '加载上下文失败');
    } finally {
      setRuntimeContextLoading(false);
    }
  }, [apiClient]);

  const handleOpenRuntimeContext = useCallback((sessionId: string) => {
    if (!sessionId) {
      return;
    }
    setRuntimeContextOpen(true);
    setRuntimeContextSessionId(sessionId);
    setRuntimeContextData(null);
    void loadLatestRuntimeContext(sessionId);
  }, [loadLatestRuntimeContext]);

  const handleRefreshRuntimeContext = useCallback(() => {
    if (!runtimeContextSessionId) {
      return;
    }
    void loadLatestRuntimeContext(runtimeContextSessionId);
  }, [loadLatestRuntimeContext, runtimeContextSessionId]);

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
    loadWorkbarSummaries: loadContactMemoryContext,
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
            onOpenSessionSummary={(sessionId) => {
              setSummaryPaneSessionId((prev) => (prev === sessionId ? null : sessionId));
            }}
            onOpenSessionRuntimeContext={handleOpenRuntimeContext}
            activeSummarySessionId={summaryPaneSessionId}
            activeRuntimeContextSessionId={runtimeContextOpen ? runtimeContextSessionId : null}
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
              <ChatConversationPane
                currentSession={currentSession}
                sessionSummaryPaneVisible={sessionSummaryPaneVisible}
                currentContactName={currentContactName}
                currentProjectNameForMemory={currentProjectNameForMemory}
                currentProjectIdForMemory={currentProjectIdForMemory || null}
                messages={messages}
                chatIsLoading={chatIsLoading}
                chatIsStreaming={chatIsStreaming}
                chatIsStopping={chatIsStopping}
                hasMoreMessages={hasMoreMessages}
                onLoadMore={handleLoadMore}
                onToggleTurnProcess={handleToggleTurnProcess}
                customRenderer={customRenderer}
                sessionMemorySummaries={sessionMemorySummaries}
                agentRecalls={agentRecalls}
                memoryLoading={memoryLoading}
                memoryError={memoryError}
                onRefreshMemory={handleRefreshMemory}
                onCloseSummary={handleCloseSummary}
                toggleSidebar={toggleSidebar}
                mergedCurrentTurnTasks={mergedCurrentTurnTasks}
                workbarHistoryTasks={workbarHistoryTasks}
                activeConversationTurnId={activeConversationTurnId}
                workbarLoading={workbarLoading}
                workbarHistoryLoading={workbarHistoryLoading}
                workbarError={workbarError}
                workbarHistoryError={workbarHistoryError}
                workbarActionLoadingTaskId={workbarActionLoadingTaskId}
                onRefreshWorkbarTasks={handleRefreshWorkbar}
                onOpenHistory={handleOpenHistory}
                onOpenUiPromptHistory={handleOpenUiPromptHistory}
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
                onGuide={handleRuntimeGuidanceSend}
                onStop={abortCurrentConversation}
                inputDisabled={chatIsStopping || !currentSession}
                isStreaming={chatIsStreaming}
                isStopping={chatIsStopping}
                supportedFileTypes={supportedFileTypes}
                supportsReasoning={supportsReasoning}
                reasoningEnabled={chatConfig?.reasoningEnabled === true}
                onReasoningToggle={(enabled) => updateChatConfig({ reasoningEnabled: enabled })}
                selectedModelId={selectedModelId}
                availableModels={aiModelConfigs}
                onModelChange={setSelectedModel}
                availableProjects={composerAvailableProjects}
                currentProject={currentProject}
                onProjectChange={handleComposerProjectChange}
                workspaceRoot={composerWorkspaceRoot}
                onWorkspaceRootChange={handleComposerWorkspaceRootChange}
                mcpEnabled={composerMcpEnabled}
                enabledMcpIds={composerEnabledMcpIds}
                onMcpEnabledChange={handleComposerMcpEnabledChange}
                onEnabledMcpIdsChange={handleComposerEnabledMcpIdsChange}
                runtimeGuidancePendingCount={Number(currentRuntimeGuidanceState?.pendingCount || 0)}
                runtimeGuidanceAppliedCount={Number(currentRuntimeGuidanceState?.appliedCount || 0)}
                runtimeGuidanceLastAppliedAt={currentRuntimeGuidanceState?.lastAppliedAt || null}
                runtimeGuidanceItems={Array.isArray(currentRuntimeGuidanceState?.items) ? currentRuntimeGuidanceState.items : []}
              />
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

        <TurnRuntimeContextDrawer
          open={runtimeContextOpen}
          sessionId={runtimeContextSessionId}
          loading={runtimeContextLoading}
          error={runtimeContextError}
          data={runtimeContextData}
          onRefresh={handleRefreshRuntimeContext}
          onClose={() => setRuntimeContextOpen(false)}
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
