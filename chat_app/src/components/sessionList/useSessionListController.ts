import { useEffect, useMemo, useState } from 'react';

import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { apiClient as globalApiClient } from '../../lib/api/client';
import { useChatApiClientFromContext, useChatStoreContext } from '../../lib/store/ChatStoreContext';
import { useChatStore } from '../../lib/store';
import {
  useContactSessionCreator,
} from './useContactSessionCreator';
import {
  useContactSessionListState,
} from './useContactSessionListState';
import { useInlineActionMenus } from './useInlineActionMenus';
import { useSectionExpansion } from './useSectionExpansion';
import { useSessionListBootstrap } from './useSessionListBootstrap';
import { useLocalFsPickers } from './useLocalFsPickers';
import { useProjectRunState } from './useProjectRunState';
import { useSessionListDeleteActions } from './useSessionListDeleteActions';
import { useSessionListActions } from './useSessionListActions';
import { useSessionListStoreState } from './useSessionListStoreState';
import { useRemoteConnectionForm } from './useRemoteConnectionForm';
import type { ContactItem } from './types';

interface SessionListControllerParams {
  store?: typeof useChatStore;
  activeSummarySessionId?: string | null;
  onOpenSessionSummary?: (sessionId: string) => void;
  onOpenSessionRuntimeContext?: (sessionId: string) => void;
  isCollapsed: boolean;
}

export const useSessionListController = ({
  store,
  activeSummarySessionId,
  onOpenSessionSummary,
  onOpenSessionRuntimeContext,
  isCollapsed,
}: SessionListControllerParams) => {
  let contextStoreHook: typeof useChatStore | null = null;
  try {
    contextStoreHook = useChatStoreContext();
  } catch {
    contextStoreHook = null;
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

  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = apiClientFromContext || globalApiClient;

  const remoteForm = useRemoteConnectionForm({
    apiClient,
    createRemoteConnection,
    updateRemoteConnection,
  });

  const localFsPickers = useLocalFsPickers({
    apiClient,
    projectRoot,
    terminalRoot,
    remotePrivateKeyPath: remoteForm.remotePrivateKeyPath,
    remoteCertificatePath: remoteForm.remoteCertificatePath,
    remoteJumpPrivateKeyPath: remoteForm.remoteJumpPrivateKeyPath,
    onProjectRootChange: setProjectRoot,
    onTerminalRootChange: setTerminalRoot,
    onRemotePrivateKeyPathChange: remoteForm.setRemotePrivateKeyPath,
    onRemoteCertificatePathChange: remoteForm.setRemoteCertificatePath,
    onRemoteJumpPrivateKeyPathChange: remoteForm.setRemoteJumpPrivateKeyPath,
  });

  const { dialogState, showConfirmDialog, handleConfirm, handleCancel } = useConfirmDialog();

  const existingContactAgentIds = useMemo(
    () => (contacts || []).map((item: ContactItem) => item.agentId),
    [contacts],
  );

  const contactSessionState = useContactSessionListState({
    contacts,
    sessions: sessions || [],
    currentSession,
    activePanel,
    activeSummarySessionId,
    createSession,
    apiClient,
  });

  const contactSessionCreator = useContactSessionCreator({
    agents: agents as any[],
    currentSessionId: currentSession?.id || null,
    loadContacts: loadContactsAction,
    createContact: createContactAction,
    ensureSessionForContact: contactSessionState.ensureSessionForContact as any,
    updateSession,
    selectSession,
  });

  const inlineActionMenus = useInlineActionMenus();

  const sessionListActions = useSessionListActions({
    contacts: contacts as ContactItem[],
    currentSession,
    terminals,
    currentTerminal,
    remoteConnections,
    currentRemoteConnection,
    ensureSessionForContact: contactSessionState.ensureSessionForContact as any,
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
    setKeyFilePickerOpen: localFsPickers.setKeyFilePickerOpen,
    openRemoteModalBase: remoteForm.openRemoteModal,
    createProject,
    createTerminal,
    selectProject,
    selectTerminal,
    selectRemoteConnection,
    openRemoteSftp,
    projectRoot,
    terminalRoot,
  });

  const sectionExpansion = useSectionExpansion({
    onFocusTerminal: sessionListActions.focusTerminalPanel,
    onFocusRemote: sessionListActions.focusRemotePanel,
  });

  const deleteActions = useSessionListDeleteActions({
    projects,
    terminals,
    remoteConnections,
    displaySessions: contactSessionState.displaySessions,
    contacts,
    currentSession,
    deleteProject,
    deleteTerminal,
    deleteRemoteConnection,
    deleteSession,
    deleteContactAction,
    loadContactsAction,
    clearCachedSessionIdsForContact: contactSessionState.clearCachedSessionIdsForContact,
    showConfirmDialog,
  });

  useSessionListBootstrap({
    loadProjects,
    loadAgents,
    loadContacts: loadContactsAction,
    loadTerminals,
    loadRemoteConnections,
    isCollapsed,
    terminalsExpanded: sectionExpansion.terminalsExpanded,
    remoteExpanded: sectionExpansion.remoteExpanded,
  });

  useEffect(() => {
    if (isCollapsed) {
      return;
    }
    const timer = setInterval(() => {
      void loadTerminals();
    }, 2000);
    return () => clearInterval(timer);
  }, [isCollapsed, loadTerminals]);

  const projectRunState = useProjectRunState({
    apiClient,
    projects,
    terminals,
    loadTerminals,
    handleSelectTerminal: sessionListActions.handleSelectTerminal,
    setActivePanel,
  });

  return {
    agents,
    apiClient,
    contactSessionCreator,
    contactSessionState,
    currentProject,
    currentRemoteConnection,
    currentTerminal,
    deleteActions,
    dialogState,
    existingContactAgentIds,
    handleCancel,
    handleConfirm,
    inlineActionMenus,
    isRefreshing,
    isRefreshingRemote,
    isRefreshingTerminals,
    localFsPickers,
    projectError,
    projectModalOpen,
    projectRoot,
    projectRunState,
    projects,
    remoteConnections,
    remoteForm,
    sectionExpansion,
    sessionChatState,
    sessionListActions,
    sessions,
    setProjectModalOpen,
    setProjectRoot,
    setTerminalModalOpen,
    setTerminalRoot,
    taskReviewPanelsBySession,
    terminals,
    terminalError,
    terminalModalOpen,
    terminalRoot,
    uiPromptPanelsBySession,
  };
};
