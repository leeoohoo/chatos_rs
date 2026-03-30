import React, { useEffect, useMemo, useState } from 'react';
import { useChatStoreContext, useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import { apiClient as globalApiClient } from '../lib/api/client';
import { useConfirmDialog } from '../hooks/useConfirmDialog';
import { cn } from '../lib/utils';
import { normalizeProjectRunCatalog } from './projectExplorer/utils';
import { SessionListDialogs } from './sessionList/SessionListDialogs';
import {
  ProjectSection,
  RemoteSection,
  SessionSection,
  TerminalSection,
} from './sessionList/Sections';
import {
  formatTimeAgo,
  getSessionStatus,
} from './sessionList/helpers';
import { useRemoteConnectionForm } from './sessionList/useRemoteConnectionForm';
import {
  useContactSessionListState,
} from './sessionList/useContactSessionListState';
import { useInlineActionMenus } from './sessionList/useInlineActionMenus';
import { useSectionExpansion } from './sessionList/useSectionExpansion';
import { useSessionListBootstrap } from './sessionList/useSessionListBootstrap';
import { useLocalFsPickers } from './sessionList/useLocalFsPickers';
import { useContactSessionCreator } from './sessionList/useContactSessionCreator';
import { useSessionListDeleteActions } from './sessionList/useSessionListDeleteActions';
import { useSessionListActions } from './sessionList/useSessionListActions';
import { useSessionListStoreState } from './sessionList/useSessionListStoreState';

type ContactItem = {
  id: string;
  agentId: string;
  name: string;
  status: string;
  createdAt: Date;
  updatedAt: Date;
};

interface SessionListProps {
  isOpen?: boolean;
  onClose?: () => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
  className?: string;
  store?: typeof useChatStore;
  onSelectSession?: (sessionId: string) => void;
  onOpenSessionSummary?: (sessionId: string) => void;
  onOpenSessionRuntimeContext?: (sessionId: string) => void;
  activeSummarySessionId?: string | null;
  activeRuntimeContextSessionId?: string | null;
}

export const SessionList: React.FC<SessionListProps> = (props) => {
  const {
    isOpen = true,
    collapsed,
    className,
    store,
    onSelectSession,
    onOpenSessionSummary,
    onOpenSessionRuntimeContext,
    activeSummarySessionId,
    activeRuntimeContextSessionId,
  } = props;
  // 尝试从Context获取store hook（如果可用）
  let contextStoreHook: typeof useChatStore | null = null;
  try {
    contextStoreHook = useChatStoreContext();
  } catch (error) {
    // 如果Context不可用，contextStoreHook保持为null
  }
  
  const storeToUse = store || contextStoreHook;
  
  if (!storeToUse) {
    throw new Error('SessionList must be used within a ChatStoreProvider or receive a store prop');
  }
  
  const {
    sessions,
    contacts,
    agents,
    currentSession,
    activePanel,
    loadContacts: loadContactsAction,
    createContact: createContactAction,
    deleteContact: deleteContactAction,
    createSession,
    selectSession,
    deleteSession,
    updateSession,
    loadAgents,
    sessionChatState,
    taskReviewPanelsBySession = {},
    uiPromptPanelsBySession = {},
    projects,
    currentProject,
    loadProjects,
    createProject,
    selectProject,
    deleteProject,
    setActivePanel,
    terminals,
    currentTerminal,
    loadTerminals,
    createTerminal,
    selectTerminal,
    deleteTerminal,
    remoteConnections,
    currentRemoteConnection,
    loadRemoteConnections,
    createRemoteConnection,
    updateRemoteConnection,
    selectRemoteConnection,
    deleteRemoteConnection,
    openRemoteSftp,
  } = useSessionListStoreState(storeToUse);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isRefreshingTerminals, setIsRefreshingTerminals] = useState(false);
  const [isRefreshingRemote, setIsRefreshingRemote] = useState(false);

  const [projectModalOpen, setProjectModalOpen] = useState(false);
  const [projectRoot, setProjectRoot] = useState('');
  const [projectError, setProjectError] = useState<string | null>(null);

  const [terminalModalOpen, setTerminalModalOpen] = useState(false);
  const [terminalRoot, setTerminalRoot] = useState('');
  const [terminalError, setTerminalError] = useState<string | null>(null);
  const [projectRunStateById, setProjectRunStateById] = useState<Record<string, {
    status: string;
    loading: boolean;
    targetCount: number;
    defaultTargetId: string | null;
    fallbackTargetId: string | null;
    error: string | null;
  }>>({});
  const [runningProjectId, setRunningProjectId] = useState<string | null>(null);

  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = apiClientFromContext || globalApiClient;

  const {
    remoteModalOpen,
    setRemoteModalOpen,
    remoteName,
    setRemoteName,
    remoteHost,
    setRemoteHost,
    remotePort,
    setRemotePort,
    remoteUsername,
    setRemoteUsername,
    remoteAuthType,
    setRemoteAuthType,
    remotePassword,
    setRemotePassword,
    remotePrivateKeyPath,
    setRemotePrivateKeyPath,
    remoteCertificatePath,
    setRemoteCertificatePath,
    remoteDefaultPath,
    setRemoteDefaultPath,
    remoteHostKeyPolicy,
    setRemoteHostKeyPolicy,
    remoteJumpEnabled,
    setRemoteJumpEnabled,
    remoteJumpHost,
    setRemoteJumpHost,
    remoteJumpPort,
    setRemoteJumpPort,
    remoteJumpUsername,
    setRemoteJumpUsername,
    remoteJumpPrivateKeyPath,
    setRemoteJumpPrivateKeyPath,
    remoteJumpPassword,
    setRemoteJumpPassword,
    remoteError,
    remoteErrorAction,
    remoteSuccess,
    remoteTesting,
    remoteSaving,
    editingRemoteConnectionId,
    openRemoteModal: openRemoteModalBase,
    openEditRemoteModal,
    handleTestRemoteConnection,
    handleSaveRemoteConnection,
    handleQuickTestRemoteConnection,
  } = useRemoteConnectionForm({
    apiClient,
    createRemoteConnection,
    updateRemoteConnection,
  });
  const {
    keyFilePickerOpen,
    keyFilePickerTitle,
    keyFilePickerPath,
    keyFilePickerParent,
    keyFilePickerLoading,
    keyFilePickerItems,
    keyFilePickerError,
    dirPickerOpen,
    dirPickerTarget,
    dirPickerPath,
    dirPickerParent,
    dirPickerLoading,
    dirPickerItems,
    dirPickerError,
    showHiddenDirs,
    dirPickerCreateModalOpen,
    dirPickerNewFolderName,
    dirPickerCreatingFolder,
    setShowHiddenDirs,
    setDirPickerCreateModalOpen,
    setDirPickerNewFolderName,
    openDirPicker,
    closeDirPicker,
    openCreateDirModal,
    createDirInPicker,
    chooseDir,
    openKeyFilePicker,
    closeKeyFilePicker,
    applySelectedKeyFile,
    loadDirEntries,
    loadKeyFileEntries,
    setKeyFilePickerOpen,
  } = useLocalFsPickers({
    apiClient,
    projectRoot,
    terminalRoot,
    remotePrivateKeyPath,
    remoteCertificatePath,
    remoteJumpPrivateKeyPath,
    onProjectRootChange: setProjectRoot,
    onTerminalRootChange: setTerminalRoot,
    onRemotePrivateKeyPathChange: setRemotePrivateKeyPath,
    onRemoteCertificatePathChange: setRemoteCertificatePath,
    onRemoteJumpPrivateKeyPathChange: setRemoteJumpPrivateKeyPath,
  });
  
  const { dialogState, showConfirmDialog, handleConfirm, handleCancel } = useConfirmDialog();

  const isCollapsed = collapsed ?? !isOpen;
  const existingContactAgentIds = useMemo(
    () => (contacts || []).map((item: ContactItem) => item.agentId),
    [contacts],
  );

  const {
    ensureSessionForContact,
    displaySessionRuntimeIdMap,
    displaySessions,
    currentDisplaySessionId,
    activeSummaryDisplaySessionId,
    clearCachedSessionIdsForContact,
  } = useContactSessionListState({
    contacts,
    sessions: sessions || [],
    currentSession,
    activePanel,
    activeSummarySessionId,
    createSession,
    apiClient,
  });

  const {
    createContactModalOpen,
    selectedContactAgentId,
    contactError,
    setSelectedContactAgentId,
    setContactError,
    openCreateSessionModal,
    closeCreateSessionModal,
    handleCreateContactSession,
  } = useContactSessionCreator({
    agents: agents as any[],
    currentSessionId: currentSession?.id || null,
    loadContacts: loadContactsAction,
    createContact: createContactAction,
    ensureSessionForContact: ensureSessionForContact as any,
    updateSession,
    selectSession,
  });

  const { closeActionMenus, toggleActionMenu } = useInlineActionMenus();
  const {
    handleSelectSession,
    handleOpenSummary,
    handleOpenRuntimeContext,
    handleRefreshSessions,
    handleRefreshTerminals,
    handleRefreshRemote,
    openProjectModal,
    openTerminalModal,
    openRemoteModal,
    handleCreateProject,
    handleCreateTerminal,
    handleSelectProject,
    handleSelectTerminal,
    handleSelectRemoteConnection,
    handleOpenRemoteSftp,
    focusTerminalPanel,
    focusRemotePanel,
  } = useSessionListActions({
    contacts: contacts as ContactItem[],
    currentSession,
    terminals,
    currentTerminal,
    remoteConnections,
    currentRemoteConnection,
    ensureSessionForContact: ensureSessionForContact as any,
    selectSession,
    setActivePanel,
    onOpenSessionSummary,
    onOpenSessionRuntimeContext,
    loadContactsAction,
    loadTerminals,
    loadRemoteConnections,
    setIsRefreshing,
    setIsRefreshingTerminals,
    setIsRefreshingRemote,
    setProjectRoot,
    setProjectError,
    setProjectModalOpen,
    setTerminalRoot,
    setTerminalError,
    setTerminalModalOpen,
    setKeyFilePickerOpen,
    openRemoteModalBase,
    createProject,
    createTerminal,
    selectProject,
    selectTerminal,
    selectRemoteConnection,
    openRemoteSftp,
    projectRoot,
    terminalRoot,
  });
  const {
    sessionsExpanded,
    projectsExpanded,
    terminalsExpanded,
    remoteExpanded,
    handleToggleSessionsSection,
    handleToggleProjectsSection,
    handleToggleTerminalsSection,
    handleToggleRemoteSection,
  } = useSectionExpansion({
    onFocusTerminal: focusTerminalPanel,
    onFocusRemote: focusRemotePanel,
  });
  const {
    handleArchiveProject,
    handleDeleteTerminal,
    handleDeleteRemoteConnection,
    handleDeleteSession,
  } = useSessionListDeleteActions({
    projects,
    terminals,
    remoteConnections,
    displaySessions,
    contacts,
    currentSession,
    deleteProject,
    deleteTerminal,
    deleteRemoteConnection,
    deleteSession,
    deleteContactAction,
    loadContactsAction,
    clearCachedSessionIdsForContact,
    showConfirmDialog,
  });

  useSessionListBootstrap({
    loadProjects,
    loadAgents,
    loadContacts: loadContactsAction,
    loadTerminals,
    loadRemoteConnections,
    isCollapsed,
    terminalsExpanded,
    remoteExpanded,
  });

  useEffect(() => {
    let cancelled = false;
    const ids = new Set((projects || []).map((project) => String(project.id || '')));

    setProjectRunStateById((prev) => {
      const next: Record<string, {
        status: string;
        loading: boolean;
        targetCount: number;
        defaultTargetId: string | null;
        fallbackTargetId: string | null;
        error: string | null;
      }> = {};
      (projects || []).forEach((project) => {
        const previous = prev[project.id];
        next[project.id] = previous || {
          status: 'analyzing',
          loading: true,
          targetCount: 0,
          defaultTargetId: null,
          fallbackTargetId: null,
          error: null,
        };
      });
      return next;
    });

    const load = async () => {
      const updates = await Promise.all(
        (projects || []).map(async (project) => {
          try {
            const raw = await apiClient.getProjectRunCatalog(project.id);
            const catalog = normalizeProjectRunCatalog(raw);
            const fallback = catalog.targets[0]?.id ? String(catalog.targets[0].id) : null;
            const targetCount = catalog.targets.length;
            const status = targetCount > 0 ? 'ready' : (catalog.status || 'empty');
            return {
              projectId: project.id,
              state: {
                status,
                loading: false,
                targetCount,
                defaultTargetId: catalog.defaultTargetId ? String(catalog.defaultTargetId) : null,
                fallbackTargetId: fallback,
                error: catalog.errorMessage ? String(catalog.errorMessage) : null,
              },
            };
          } catch (err: any) {
            return {
              projectId: project.id,
              state: {
                status: 'error',
                loading: false,
                targetCount: 0,
                defaultTargetId: null,
                fallbackTargetId: null,
                error: err?.message || '运行目标加载失败',
              },
            };
          }
        })
      );

      if (cancelled) return;
      setProjectRunStateById((prev) => {
        const next: Record<string, typeof updates[number]['state']> = {};
        ids.forEach((id) => {
          const old = prev[id];
          if (old) {
            next[id] = old;
          }
        });
        updates.forEach((item) => {
          next[item.projectId] = item.state;
        });
        return next;
      });
    };

    void load();

    return () => {
      cancelled = true;
    };
  }, [apiClient, projects]);

  const handleRunProject = async (projectId: string) => {
    const project = (projects || []).find((item) => item.id === projectId);
    if (!project) return;
    const runState = projectRunStateById[projectId];
    if (!runState) return;
    const targetId = runState.defaultTargetId || runState.fallbackTargetId;
    if (!targetId) return;

    setRunningProjectId(projectId);
    try {
      const result = await apiClient.executeProjectRun(projectId, {
        target_id: targetId,
        create_if_missing: true,
      });
      const terminalId = String(result?.terminal_id || '').trim();
      if (terminalId) {
        await handleSelectTerminal(terminalId);
      }
      setActivePanel('terminal');
    } finally {
      setRunningProjectId((prev) => (prev === projectId ? null : prev));
    }
  };

  return (
    <div
      className={cn(
        'flex flex-col h-full bg-card transition-all duration-200 overflow-hidden',
        isCollapsed ? 'w-0' : 'w-64 sm:w-72 border-r border-border',
        className
      )}
    >
      {/* 联系人与项目列表 */}
      {!isCollapsed && (
        <div className="flex-1 flex flex-col overflow-hidden">
          <SessionSection
            expanded={sessionsExpanded}
            sessions={displaySessions}
            currentSessionId={currentDisplaySessionId}
            summarySessionId={activeSummaryDisplaySessionId}
            runtimeContextSessionId={activeRuntimeContextSessionId}
            displaySessionRuntimeIdMap={displaySessionRuntimeIdMap}
            sessionChatState={sessionChatState}
            taskReviewPanelsBySession={taskReviewPanelsBySession}
            uiPromptPanelsBySession={uiPromptPanelsBySession}
            hasMore={false}
            isRefreshing={isRefreshing}
            isLoadingMore={false}
            onToggle={handleToggleSessionsSection}
            onRefresh={handleRefreshSessions}
            onCreateSession={() => {
              void openCreateSessionModal();
            }}
            onSelectSession={(sessionId) => {
              void handleSelectSession(sessionId).then((selectedSessionId) => {
                if (selectedSessionId) {
                  onSelectSession?.(selectedSessionId);
                }
              });
            }}
            onOpenSummary={handleOpenSummary}
            onOpenRuntimeContext={handleOpenRuntimeContext}
            onDeleteSession={handleDeleteSession}
            onLoadMore={() => {}}
            onToggleActionMenu={toggleActionMenu}
            closeActionMenus={() => closeActionMenus()}
            formatTimeAgo={formatTimeAgo}
            getSessionStatus={getSessionStatus}
          />

          <div className="my-2 border-t border-border" />

          <ProjectSection
            expanded={projectsExpanded}
            projects={projects}
            currentProjectId={currentProject?.id}
            projectRunStateById={projectRunStateById}
            runningProjectId={runningProjectId}
            onToggle={handleToggleProjectsSection}
            onCreate={openProjectModal}
            onSelect={(projectId) => {
              void handleSelectProject(projectId);
            }}
            onRunProject={(project) => {
              void handleRunProject(project.id);
            }}
            onArchive={(projectId) => {
              void handleArchiveProject(projectId);
            }}
            onToggleActionMenu={toggleActionMenu}
            closeActionMenus={() => closeActionMenus()}
          />

          <div className="my-2 border-t border-border" />

          <TerminalSection
            expanded={terminalsExpanded}
            terminals={terminals}
            currentTerminalId={currentTerminal?.id}
            isRefreshing={isRefreshingTerminals}
            onToggle={handleToggleTerminalsSection}
            onRefresh={handleRefreshTerminals}
            onCreate={openTerminalModal}
            onSelect={(terminalId) => {
              void handleSelectTerminal(terminalId);
            }}
            onDelete={handleDeleteTerminal}
            onToggleActionMenu={toggleActionMenu}
            closeActionMenus={() => closeActionMenus()}
            formatTimeAgo={formatTimeAgo}
          />

          <div className="my-2 border-t border-border" />

          <RemoteSection
            expanded={remoteExpanded}
            remoteConnections={remoteConnections}
            currentRemoteConnectionId={currentRemoteConnection?.id}
            isRefreshing={isRefreshingRemote}
            onToggle={handleToggleRemoteSection}
            onRefresh={handleRefreshRemote}
            onCreate={openRemoteModal}
            onSelect={(connectionId) => {
              void handleSelectRemoteConnection(connectionId);
            }}
            onOpenSftp={(connectionId) => {
              void handleOpenRemoteSftp(connectionId);
            }}
            onEdit={(connection) => {
              setKeyFilePickerOpen(false);
              openEditRemoteModal(connection);
            }}
            onTest={handleQuickTestRemoteConnection}
            onDelete={handleDeleteRemoteConnection}
            onToggleActionMenu={toggleActionMenu}
            closeActionMenus={() => closeActionMenus()}
            formatTimeAgo={formatTimeAgo}
          />
        </div>
      )}
      <SessionListDialogs
        createContactModalOpen={createContactModalOpen}
        agents={(agents || []) as any[]}
        existingContactAgentIds={existingContactAgentIds}
        selectedContactAgentId={selectedContactAgentId}
        contactError={contactError}
        closeCreateSessionModal={closeCreateSessionModal}
        setSelectedContactAgentId={setSelectedContactAgentId}
        setContactError={setContactError}
        handleCreateContactSession={handleCreateContactSession}
        projectModalOpen={projectModalOpen}
        projectRoot={projectRoot}
        projectError={projectError}
        setProjectModalOpen={setProjectModalOpen}
        setProjectRoot={setProjectRoot}
        openDirPickerForProject={() => {
          void openDirPicker('project');
        }}
        handleCreateProject={handleCreateProject}
        terminalModalOpen={terminalModalOpen}
        terminalRoot={terminalRoot}
        terminalError={terminalError}
        setTerminalModalOpen={setTerminalModalOpen}
        setTerminalRoot={setTerminalRoot}
        openDirPickerForTerminal={() => {
          void openDirPicker('terminal');
        }}
        handleCreateTerminal={handleCreateTerminal}
        remoteModalOpen={remoteModalOpen}
        editingRemoteConnectionId={editingRemoteConnectionId}
        remoteName={remoteName}
        remoteHost={remoteHost}
        remotePort={remotePort}
        remoteUsername={remoteUsername}
        remoteAuthType={remoteAuthType}
        remotePassword={remotePassword}
        remotePrivateKeyPath={remotePrivateKeyPath}
        remoteCertificatePath={remoteCertificatePath}
        remoteDefaultPath={remoteDefaultPath}
        remoteHostKeyPolicy={remoteHostKeyPolicy}
        remoteJumpEnabled={remoteJumpEnabled}
        remoteJumpHost={remoteJumpHost}
        remoteJumpPort={remoteJumpPort}
        remoteJumpUsername={remoteJumpUsername}
        remoteJumpPrivateKeyPath={remoteJumpPrivateKeyPath}
        remoteJumpPassword={remoteJumpPassword}
        remoteError={remoteError}
        remoteErrorAction={remoteErrorAction}
        remoteSuccess={remoteSuccess}
        remoteTesting={remoteTesting}
        remoteSaving={remoteSaving}
        setRemoteModalOpen={setRemoteModalOpen}
        setRemoteName={setRemoteName}
        setRemoteHost={setRemoteHost}
        setRemotePort={setRemotePort}
        setRemoteUsername={setRemoteUsername}
        setRemoteAuthType={setRemoteAuthType}
        setRemotePassword={setRemotePassword}
        setRemotePrivateKeyPath={setRemotePrivateKeyPath}
        setRemoteCertificatePath={setRemoteCertificatePath}
        setRemoteDefaultPath={setRemoteDefaultPath}
        setRemoteHostKeyPolicy={setRemoteHostKeyPolicy}
        setRemoteJumpEnabled={setRemoteJumpEnabled}
        setRemoteJumpHost={setRemoteJumpHost}
        setRemoteJumpPort={setRemoteJumpPort}
        setRemoteJumpUsername={setRemoteJumpUsername}
        setRemoteJumpPrivateKeyPath={setRemoteJumpPrivateKeyPath}
        setRemoteJumpPassword={setRemoteJumpPassword}
        openKeyFilePicker={openKeyFilePicker}
        handleTestRemoteConnection={handleTestRemoteConnection}
        handleSaveRemoteConnection={handleSaveRemoteConnection}
        keyFilePickerOpen={keyFilePickerOpen}
        keyFilePickerTitle={keyFilePickerTitle}
        keyFilePickerPath={keyFilePickerPath}
        keyFilePickerParent={keyFilePickerParent}
        keyFilePickerLoading={keyFilePickerLoading}
        keyFilePickerItems={keyFilePickerItems}
        keyFilePickerError={keyFilePickerError}
        closeKeyFilePicker={closeKeyFilePicker}
        loadKeyFileEntries={loadKeyFileEntries}
        applySelectedKeyFile={applySelectedKeyFile}
        dirPickerOpen={dirPickerOpen}
        dirPickerTarget={dirPickerTarget}
        dirPickerPath={dirPickerPath}
        dirPickerParent={dirPickerParent}
        dirPickerLoading={dirPickerLoading}
        dirPickerItems={dirPickerItems}
        dirPickerError={dirPickerError}
        showHiddenDirs={showHiddenDirs}
        dirPickerCreateModalOpen={dirPickerCreateModalOpen}
        dirPickerNewFolderName={dirPickerNewFolderName}
        dirPickerCreatingFolder={dirPickerCreatingFolder}
        closeDirPicker={closeDirPicker}
        chooseDir={chooseDir}
        openCreateDirModal={openCreateDirModal}
        setShowHiddenDirs={setShowHiddenDirs}
        loadDirEntries={loadDirEntries}
        setDirPickerCreateModalOpen={setDirPickerCreateModalOpen}
        setDirPickerNewFolderName={setDirPickerNewFolderName}
        createDirInPicker={createDirInPicker}
        dialogState={dialogState}
        handleConfirm={handleConfirm}
        handleCancel={handleCancel}
      />
    </div>
  );
};
