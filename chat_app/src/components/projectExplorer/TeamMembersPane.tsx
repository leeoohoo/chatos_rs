import React, { useCallback, useMemo } from 'react';
import { shallow } from 'zustand/shallow';

import { ProjectContactPickerModal } from '../sessionList/ProjectContactPickerModal';
import { apiClient as globalApiClient } from '../../lib/api/client';
import { useChatApiClientFromContext, useChatStoreSelector } from '../../lib/store/ChatStoreContext';
import { cn } from '../../lib/utils';
import type { Project, Session } from '../../types';
import {
  findLatestMatchedSession,
  normalizeProjectScopeId,
  resolveSessionProjectScopeId,
  resolveSessionTimestamp,
} from '../../features/contactSession/sessionResolver';
import { useContactSessionResolver } from '../../features/contactSession/useContactSessionResolver';
import { useSessionRuntimeSettings } from '../../features/sessionRuntime/useSessionRuntimeSettings';
import {
  useSessionSummaryPanel,
} from '../../features/sessionSummary/useSessionSummaryPanel';
import { useSessionWorkbarPanels } from '../chatInterface/useSessionWorkbarPanels';
import type {
  ContactItem,
  ProjectContactRow,
} from './teamMembers/types';
import TurnRuntimeContextDrawer from '../chatInterface/TurnRuntimeContextDrawer';
import TeamMembersSidebar from './teamMembers/TeamMembersSidebar';
import TeamMemberWorkspace from './teamMembers/TeamMemberWorkspace';
import { useTeamMemberConversation } from './teamMembers/useTeamMemberConversation';
import { useProjectMembersManager } from './teamMembers/useProjectMembersManager';
import { useTeamMemberRuntimeContext } from './teamMembers/useTeamMemberRuntimeContext';

interface TeamMembersPaneProps {
  project: Project;
  className?: string;
}

const TeamMembersPane: React.FC<TeamMembersPaneProps> = ({ project, className }) => {
  const {
    currentSession,
    sessions,
    contacts,
    remoteConnections,
    currentRemoteConnection,
    loadContacts,
    messages,
    hasMoreMessages,
    sessionChatState,
    sendMessage,
    selectRemoteConnection,
    abortCurrentConversation,
    loadMoreMessages,
    toggleTurnProcess,
    createSession,
    selectSession,
    updateSession,
    aiModelConfigs,
    selectedModelId,
    setSelectedModel,
    chatConfig,
    updateChatConfig,
    submitRuntimeGuidance,
    sessionRuntimeGuidanceState,
    taskReviewPanelsBySession,
    uiPromptPanelsBySession,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
  } = useChatStoreSelector((state) => ({
    currentSession: state.currentSession,
    sessions: state.sessions,
    contacts: state.contacts,
    remoteConnections: state.remoteConnections,
    currentRemoteConnection: state.currentRemoteConnection,
    loadContacts: state.loadContacts,
    messages: state.messages,
    hasMoreMessages: state.hasMoreMessages,
    sessionChatState: state.sessionChatState,
    sendMessage: state.sendMessage,
    selectRemoteConnection: state.selectRemoteConnection,
    abortCurrentConversation: state.abortCurrentConversation,
    loadMoreMessages: state.loadMoreMessages,
    toggleTurnProcess: state.toggleTurnProcess,
    createSession: state.createSession,
    selectSession: state.selectSession,
    updateSession: state.updateSession,
    aiModelConfigs: state.aiModelConfigs,
    selectedModelId: state.selectedModelId,
    setSelectedModel: state.setSelectedModel,
    chatConfig: state.chatConfig,
    updateChatConfig: state.updateChatConfig,
    submitRuntimeGuidance: state.submitRuntimeGuidance,
    sessionRuntimeGuidanceState: state.sessionRuntimeGuidanceState,
    taskReviewPanelsBySession: state.taskReviewPanelsBySession,
    uiPromptPanelsBySession: state.uiPromptPanelsBySession,
    upsertTaskReviewPanel: state.upsertTaskReviewPanel,
    removeTaskReviewPanel: state.removeTaskReviewPanel,
    upsertUiPromptPanel: state.upsertUiPromptPanel,
    removeUiPromptPanel: state.removeUiPromptPanel,
  }), shallow);
  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = useMemo(
    () => apiClientFromContext || globalApiClient,
    [apiClientFromContext],
  );

  const normalizedProjectId = normalizeProjectScopeId(project?.id || null);
  const normalizedContacts = useMemo<ContactItem[]>(() => (
    (contacts || [])
      .map((item) => {
        const id = typeof item?.id === 'string' ? item.id.trim() : '';
        const agentId = typeof item?.agentId === 'string' ? item.agentId.trim() : '';
        if (!id || !agentId) {
          return null;
        }
        return {
          id,
          agentId,
          name: typeof item?.name === 'string' && item.name.trim() ? item.name.trim() : id,
        };
      })
      .filter((item: ContactItem | null): item is ContactItem => Boolean(item))
  ), [contacts]);
  const {
    projectMembers,
    projectMembersLoading,
    projectMembersError,
    projectContactIdSet,
    memberPickerOpen,
    memberPickerSelectedId,
    memberPickerError,
    removingContactId,
    openAddMember: openAddMemberFromManager,
    confirmAddMember: confirmAddMemberFromManager,
    removeMember: removeMemberFromManager,
    closeMemberPicker,
    selectMemberPickerContact,
  } = useProjectMembersManager({
    apiClient,
    projectId: project?.id,
    contacts: normalizedContacts,
    loadContacts,
  });

  const findProjectSessionForContact = useCallback((contact: ContactItem): Session | null => {
    return findLatestMatchedSession(sessions || [], contact, normalizedProjectId);
  }, [normalizedProjectId, sessions]);
  const { ensureContactSession: ensureContactSessionFromResolver } = useContactSessionResolver({
    sessions: sessions || [],
    currentSession,
    createSession,
    apiClient,
    defaultProjectId: normalizedProjectId,
  });

  const projectContacts = useMemo<ProjectContactRow[]>(() => {
    const rows = projectMembers.map((member) => {
      const contact: ContactItem = {
        id: member.contactId,
        agentId: member.agentId,
        name: member.name,
      };
        const session = findProjectSessionForContact(contact);
        return {
          contact,
          session,
          updatedAt: session ? resolveSessionTimestamp(session) : member.updatedAt,
        };
      });
    rows.sort((a, b) => b.updatedAt - a.updatedAt);
    return rows;
  }, [findProjectSessionForContact, projectMembers]);
  const projectContactsOptions = useMemo(() => normalizedContacts, [normalizedContacts]);

  const {
    summaryPaneSessionId,
    summaryItems,
    summaryLoading,
    summaryError,
    clearingSummaries,
    deletingSummaryId,
    setSummaryPaneSessionId,
    setSummaryError,
    resetSummaryState,
    loadSessionSummaries,
    openSummaryForSession,
    deleteSummary,
    clearSummaries,
  } = useSessionSummaryPanel(apiClient);

  const ensureContactSession = useCallback(async (contact: ContactItem): Promise<string | null> => {
    const sessionId = await ensureContactSessionFromResolver(contact, {
      projectId: normalizedProjectId,
      title: contact.name || '联系人',
      selectedModelId: selectedModelId ?? null,
      projectRoot: project.rootPath || null,
      mcpEnabled: true,
      enabledMcpIds: [],
      createSessionOptions: { keepActivePanel: true },
    });

    if (sessionId && currentSession?.id !== sessionId) {
      await selectSession(sessionId, { keepActivePanel: true });
    }
    return sessionId;
  }, [
    currentSession?.id,
    ensureContactSessionFromResolver,
    normalizedProjectId,
    project.rootPath,
    selectSession,
    selectedModelId,
  ]);
  const {
    selectedContactId,
    switchingContactId,
    openingSummaryContactId,
    selectedContact,
    selectedProjectSession,
    isSelectedSessionActive,
    sessionSummaryPaneVisible,
    chatIsLoading,
    chatIsStreaming,
    chatIsStopping,
    supportsReasoning,
    setSelectedContactId,
    handleSelectContact,
    handleLoadMore,
    handleToggleTurnProcess,
    handleSendMessage,
    handleOpenSummary,
    handleDeleteSummary,
    handleClearSummaries,
  } = useTeamMemberConversation({
    projectId: normalizedProjectId,
    projectRootPath: project.rootPath || null,
    currentSession,
    projectContacts,
    normalizedContacts,
    selectedModelId,
    aiModelConfigs,
    sessionChatState,
    summaryPaneSessionId,
    setSummaryPaneSessionId,
    setSummaryError,
    resetSummaryState,
    openSummaryForSession,
    deleteSummary,
    clearSummaries,
    loadSessionSummaries,
    ensureContactSession,
    sendMessage,
    toggleTurnProcess,
    loadMoreMessages,
  });

  const selectedSessionActiveTurnId = useMemo(() => {
    if (!selectedProjectSession?.id) {
      return null;
    }
    const raw = sessionChatState?.[selectedProjectSession.id]?.activeTurnId;
    const normalized = typeof raw === 'string' ? raw.trim() : '';
    return normalized || null;
  }, [selectedProjectSession?.id, sessionChatState]);

  const runtimeSourceSession = selectedProjectSession || currentSession;
  const {
    mcpEnabled: composerMcpEnabled,
    enabledMcpIds: composerEnabledMcpIds,
    setMcpEnabled: handleComposerMcpEnabledChange,
    setEnabledMcpIds: handleComposerEnabledMcpIdsChange,
  } = useSessionRuntimeSettings({
    session: runtimeSourceSession,
    updateSession,
  });

  const handleOpenAddMember = useCallback(async () => {
    await openAddMemberFromManager();
  }, [openAddMemberFromManager]);

  const loadWorkbarSummaries = useCallback(async (sessionId: string, _force = false) => {
    if (!sessionId) {
      return;
    }
    await loadSessionSummaries(sessionId, { silent: true });
  }, [loadSessionSummaries]);

  const {
    activeConversationTurnId,
    activeTaskReviewPanel,
    activeUiPromptPanel,
    handleOpenWorkbarHistory,
    handleRefreshWorkbar,
    handleTaskReviewCancel,
    handleTaskReviewConfirm,
    handleUiPromptCancel,
    handleUiPromptSubmit,
    handleWorkbarCompleteTask,
    handleWorkbarDeleteTask,
    handleWorkbarEditTask,
    mergedCurrentTurnTasks,
    runtimeGuidanceAppliedCount,
    runtimeGuidanceItems,
    runtimeGuidanceLastAppliedAt,
    runtimeGuidancePendingCount,
    workbarActionLoadingTaskId,
    workbarError,
    workbarHistoryError,
    workbarHistoryLoading,
    workbarHistoryTasks,
    workbarLoading,
  } = useSessionWorkbarPanels({
    apiClient,
    session: isSelectedSessionActive ? selectedProjectSession : null,
    enabled: Boolean(isSelectedSessionActive && selectedProjectSession?.id),
    messages,
    selectedSessionActiveTurnId,
    sessionRuntimeGuidanceState,
    taskReviewPanelsBySession,
    uiPromptPanelsBySession,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
    loadWorkbarSummaries,
  });

  const handleOpenTeamWorkbarHistory = useCallback((sessionId: string) => {
    if (!sessionId) {
      return;
    }
    setSummaryPaneSessionId(sessionId);
    handleOpenWorkbarHistory(sessionId, { forceHistory: true, forceSummaries: true });
  }, [handleOpenWorkbarHistory, setSummaryPaneSessionId]);

  const handleConfirmAddMember = useCallback(async () => {
    const contactId = await confirmAddMemberFromManager();
    if (contactId) {
      await handleSelectContact(contactId);
    }
  }, [confirmAddMemberFromManager, handleSelectContact]);
  const {
    runtimeContextOpen,
    setRuntimeContextOpen,
    runtimeContextSessionId,
    runtimeContextData,
    runtimeContextLoading,
    runtimeContextError,
    openingRuntimeContextContactId,
    handleOpenRuntimeContext,
    handleRefreshRuntimeContext,
  } = useTeamMemberRuntimeContext({
    apiClient,
    sessions: sessions || [],
    normalizedProjectId,
    ensureContactSession,
    setSelectedContactId,
  });

  const handleRuntimeGuidanceSend = useCallback(async (content: string) => {
    if (!selectedProjectSession) {
      return;
    }
    if (resolveSessionProjectScopeId(selectedProjectSession) !== normalizedProjectId) {
      console.warn('Blocked runtime guidance for cross-project session in team pane.');
      return;
    }
    const sessionId = selectedProjectSession.id;
    const turnId = String(sessionChatState?.[sessionId]?.activeTurnId || '').trim();
    if (!sessionId || !turnId) {
      return;
    }
    try {
      await submitRuntimeGuidance(content, { sessionId, turnId, projectId: normalizedProjectId });
    } catch (error) {
      console.error('Failed to submit runtime guidance in team pane:', error);
    }
  }, [
    normalizedProjectId,
    selectedProjectSession?.id,
    selectedProjectSession,
    sessionChatState,
    submitRuntimeGuidance,
  ]);

  const handleComposerRemoteConnectionChange = useCallback((connectionId: string | null) => {
    void selectRemoteConnection(connectionId, { activatePanel: false });
  }, [selectRemoteConnection]);

  const handleRemoveMember = useCallback(async (contact: ContactItem) => {
    const targetSessionId = projectContacts.find((item) => item.contact.id === contact.id)?.session?.id || null;
    const removed = await removeMemberFromManager(contact);
    if (!removed) {
      return;
    }
    if (selectedContactId === contact.id) {
      setSelectedContactId(null);
      setSummaryPaneSessionId(null);
      resetSummaryState();
    }
    if (targetSessionId && runtimeContextSessionId === targetSessionId) {
      setRuntimeContextOpen(false);
    }
  }, [
    projectContacts,
    removeMemberFromManager,
    resetSummaryState,
    runtimeContextSessionId,
    selectedContactId,
  ]);

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        请选择一个项目
      </div>
    );
  }

  return (
    <div className={cn('flex h-full overflow-hidden', className)}>
      <TeamMembersSidebar
        projectName={project.name}
        projectMembersLoading={projectMembersLoading}
        projectMembersError={projectMembersError}
        memberPickerError={memberPickerError}
        projectContacts={projectContacts}
        selectedContactId={selectedContactId}
        switchingContactId={switchingContactId}
        summaryPaneSessionId={summaryPaneSessionId}
        openingSummaryContactId={openingSummaryContactId}
        runtimeContextSessionId={runtimeContextOpen ? runtimeContextSessionId : null}
        openingRuntimeContextContactId={openingRuntimeContextContactId}
        removingContactId={removingContactId}
        sessionChatState={sessionChatState}
        onOpenAddMember={() => { void handleOpenAddMember(); }}
        onSelectContact={(contactId) => { void handleSelectContact(contactId); }}
        onOpenSummary={(contact) => { void handleOpenSummary(contact); }}
        onOpenRuntimeContext={(contact) => { void handleOpenRuntimeContext(contact); }}
        onRemoveMember={(contact) => { void handleRemoveMember(contact); }}
      />

      <TeamMemberWorkspace
        project={project}
        selectedContact={selectedContact}
        selectedProjectSession={selectedProjectSession}
        isSelectedSessionActive={isSelectedSessionActive}
        sessionSummaryPaneVisible={sessionSummaryPaneVisible}
        summaryItems={summaryItems}
        summaryLoading={summaryLoading}
        summaryError={summaryError}
        clearingSummaries={clearingSummaries}
        deletingSummaryId={deletingSummaryId}
        messages={messages}
        hasMoreMessages={hasMoreMessages}
        chatIsLoading={chatIsLoading}
        chatIsStreaming={chatIsStreaming}
        chatIsStopping={chatIsStopping}
        selectedModelId={selectedModelId}
        aiModelConfigs={aiModelConfigs}
        supportsReasoning={supportsReasoning}
        reasoningEnabled={chatConfig?.reasoningEnabled === true}
        mcpEnabled={composerMcpEnabled}
        enabledMcpIds={composerEnabledMcpIds}
        availableRemoteConnections={remoteConnections || []}
        currentRemoteConnectionId={currentRemoteConnection?.id || null}
        onRemoteConnectionChange={handleComposerRemoteConnectionChange}
        onLoadMore={handleLoadMore}
        onToggleTurnProcess={handleToggleTurnProcess}
        onClearSummaries={() => { void handleClearSummaries(); }}
        onRefreshSummaries={() => {
          if (!selectedProjectSession?.id) {
            return;
          }
          void loadSessionSummaries(selectedProjectSession.id);
        }}
        onCloseSummary={() => setSummaryPaneSessionId(null)}
        onDeleteSummary={(summaryId) => { void handleDeleteSummary(summaryId); }}
        onSend={handleSendMessage}
        onGuide={handleRuntimeGuidanceSend}
        onStop={abortCurrentConversation}
        onModelChange={setSelectedModel}
        onReasoningToggle={(enabled) => updateChatConfig({ reasoningEnabled: enabled })}
        onMcpEnabledChange={handleComposerMcpEnabledChange}
        onEnabledMcpIdsChange={handleComposerEnabledMcpIdsChange}
        mergedCurrentTurnTasks={mergedCurrentTurnTasks}
        workbarHistoryTasks={workbarHistoryTasks}
        activeConversationTurnId={activeConversationTurnId}
        workbarLoading={workbarLoading}
        workbarHistoryLoading={workbarHistoryLoading}
        workbarError={workbarError}
        workbarHistoryError={workbarHistoryError}
        workbarActionLoadingTaskId={workbarActionLoadingTaskId}
        onRefreshWorkbarTasks={handleRefreshWorkbar}
        onOpenWorkbarHistory={handleOpenTeamWorkbarHistory}
        onCompleteTask={(task) => { void handleWorkbarCompleteTask(task); }}
        onDeleteTask={(task) => { void handleWorkbarDeleteTask(task); }}
        onEditTask={(task) => { void handleWorkbarEditTask(task); }}
        activeUiPromptPanel={activeUiPromptPanel}
        onUiPromptSubmit={handleUiPromptSubmit}
        onUiPromptCancel={handleUiPromptCancel}
        activeTaskReviewPanel={activeTaskReviewPanel}
        onTaskReviewConfirm={handleTaskReviewConfirm}
        onTaskReviewCancel={handleTaskReviewCancel}
        runtimeGuidancePendingCount={runtimeGuidancePendingCount}
        runtimeGuidanceAppliedCount={runtimeGuidanceAppliedCount}
        runtimeGuidanceLastAppliedAt={runtimeGuidanceLastAppliedAt}
        runtimeGuidanceItems={runtimeGuidanceItems}
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
      <ProjectContactPickerModal
        isOpen={memberPickerOpen}
        projectName={project.name}
        contacts={projectContactsOptions}
        disabledContactIds={Array.from(projectContactIdSet)}
        selectedContactId={memberPickerSelectedId}
        error={memberPickerError}
        onClose={closeMemberPicker}
        onSelectedContactChange={selectMemberPickerContact}
        onConfirm={() => {
          void handleConfirmAddMember();
        }}
      />
    </div>
  );
};

export default TeamMembersPane;
