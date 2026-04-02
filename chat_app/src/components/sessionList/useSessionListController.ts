import { useEffect, useMemo, useState } from 'react';

import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { apiClient as globalApiClient } from '../../lib/api/client';
import { useChatApiClientFromContext, useChatStoreContext } from '../../lib/store/ChatStoreContext';
import { useChatStore } from '../../lib/store';
import {
  useContactScopeCreator,
} from './useContactSessionCreator';
import {
  useContactScopeListState,
} from './useContactSessionListState';
import {
  CONTACT_TASK_AUTHORIZABLE_BUILTIN_MCP_ID_SET,
} from './ContactBuiltinMcpGrantsModal';
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
  const [builtinMcpGrantsModalOpen, setBuiltinMcpGrantsModalOpen] = useState(false);
  const [builtinMcpGrantsContactId, setBuiltinMcpGrantsContactId] = useState<string | null>(null);
  const [builtinMcpGrantsContactName, setBuiltinMcpGrantsContactName] = useState('');
  const [builtinMcpGrantsSelectedIds, setBuiltinMcpGrantsSelectedIds] = useState<string[]>([]);
  const [builtinMcpGrantsLoading, setBuiltinMcpGrantsLoading] = useState(false);
  const [builtinMcpGrantsSaving, setBuiltinMcpGrantsSaving] = useState(false);
  const [builtinMcpGrantsError, setBuiltinMcpGrantsError] = useState<string | null>(null);

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

  const contactScopeState = useContactScopeListState({
    contacts,
    sessions: sessions || [],
    currentSession,
    activePanel,
    activeSummarySessionId,
    createSession,
    apiClient,
  });

  const contactScopeCreator = useContactScopeCreator({
    agents: agents as any[],
    currentSessionId: currentSession?.id || null,
    loadContacts: loadContactsAction,
    createContact: createContactAction,
    ensureBackingSessionForContactScope: contactScopeState.ensureBackingSessionForContactScope as any,
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
    ensureBackingSessionForContactScope: contactScopeState.ensureBackingSessionForContactScope as any,
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
    displaySessions: contactScopeState.displayScopeSessions,
    contacts,
    currentSession,
    deleteProject,
    deleteTerminal,
    deleteRemoteConnection,
    deleteSession,
    deleteContactAction,
    loadContactsAction,
    clearCachedSessionIdsForContact: contactScopeState.clearCachedBackingSessionIdsForContact,
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

  const openBuiltinMcpGrantsModal = async (displaySessionId: string) => {
    const contactId = displaySessionId.startsWith('contact-placeholder:')
      ? displaySessionId.replace('contact-placeholder:', '').trim()
      : displaySessionId.trim();
    if (!contactId) {
      return;
    }
    const matchedContact = (contacts as ContactItem[]).find((item) => item.id === contactId);
    setBuiltinMcpGrantsModalOpen(true);
    setBuiltinMcpGrantsContactId(contactId);
    setBuiltinMcpGrantsContactName(matchedContact?.name || '联系人');
    setBuiltinMcpGrantsSelectedIds(
      (matchedContact?.authorizedBuiltinMcpIds || [])
        .filter((item) => CONTACT_TASK_AUTHORIZABLE_BUILTIN_MCP_ID_SET.has(item)),
    );
    setBuiltinMcpGrantsError(null);
    setBuiltinMcpGrantsLoading(true);
    try {
      const result = await apiClient.getContactBuiltinMcpGrants(contactId);
      setBuiltinMcpGrantsSelectedIds(
        Array.isArray(result?.authorized_builtin_mcp_ids)
          ? result.authorized_builtin_mcp_ids.filter((item: string) =>
            CONTACT_TASK_AUTHORIZABLE_BUILTIN_MCP_ID_SET.has(item))
          : [],
      );
    } catch (error) {
      setBuiltinMcpGrantsError(error instanceof Error ? error.message : '加载联系人内置 MCP 授权失败');
    } finally {
      setBuiltinMcpGrantsLoading(false);
    }
  };

  const closeBuiltinMcpGrantsModal = () => {
    if (builtinMcpGrantsSaving) {
      return;
    }
    setBuiltinMcpGrantsModalOpen(false);
    setBuiltinMcpGrantsContactId(null);
    setBuiltinMcpGrantsContactName('');
    setBuiltinMcpGrantsSelectedIds([]);
    setBuiltinMcpGrantsLoading(false);
    setBuiltinMcpGrantsError(null);
  };

  const toggleBuiltinMcpGrant = (mcpId: string) => {
    if (!mcpId || !CONTACT_TASK_AUTHORIZABLE_BUILTIN_MCP_ID_SET.has(mcpId)) {
      return;
    }
    setBuiltinMcpGrantsSelectedIds((current) => (
      current.includes(mcpId)
        ? current.filter((item) => item !== mcpId)
        : [...current, mcpId]
    ));
  };

  const saveBuiltinMcpGrants = async () => {
    if (!builtinMcpGrantsContactId) {
      return;
    }
    const nextIds = Array.from(new Set(
      builtinMcpGrantsSelectedIds.filter((item) =>
        CONTACT_TASK_AUTHORIZABLE_BUILTIN_MCP_ID_SET.has(item)),
    ));
    setBuiltinMcpGrantsSaving(true);
    setBuiltinMcpGrantsError(null);
    try {
      await apiClient.updateContactBuiltinMcpGrants(builtinMcpGrantsContactId, {
        authorized_builtin_mcp_ids: nextIds,
      });
      await loadContactsAction();
      setBuiltinMcpGrantsSelectedIds(nextIds);
      setBuiltinMcpGrantsModalOpen(false);
    } catch (error) {
      setBuiltinMcpGrantsError(error instanceof Error ? error.message : '保存联系人内置 MCP 授权失败');
    } finally {
      setBuiltinMcpGrantsSaving(false);
    }
  };

  return {
    agents,
    apiClient,
    contactScopeCreator,
    contactScopeState,
    contactSessionCreator: contactScopeCreator,
    contactSessionState: contactScopeState,
    currentProject,
    currentRemoteConnection,
    currentTerminal,
    deleteActions,
    dialogState,
    existingContactAgentIds,
    handleCancel,
    handleConfirm,
    builtinMcpGrantsContactName,
    builtinMcpGrantsError,
    builtinMcpGrantsLoading,
    builtinMcpGrantsModalOpen,
    builtinMcpGrantsSaving,
    builtinMcpGrantsSelectedIds,
    closeBuiltinMcpGrantsModal,
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
    saveBuiltinMcpGrants,
    sectionExpansion,
    sessionChatState,
    sessionListActions,
    sessions,
    setProjectModalOpen,
    setProjectRoot,
    setTerminalModalOpen,
    setTerminalRoot,
    toggleBuiltinMcpGrant,
    taskReviewPanelsBySession,
    terminals,
    terminalError,
    terminalModalOpen,
    terminalRoot,
    openBuiltinMcpGrantsModal,
    uiPromptPanelsBySession,
  };
};
