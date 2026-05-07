import { useMemo, useState } from 'react';

import { apiClient as globalApiClient } from '../../lib/api/client';
import {
  useChatApiClientFromContext,
  useOptionalChatStoreContext,
} from '../../lib/store/ChatStoreContext';
import { useChatStore } from '../../lib/store';
import { useDialogService } from '../ui/DialogProvider';
import {
  useContactSessionCreator,
} from './useContactSessionCreator';
import { useTerminalListRealtime } from '../../lib/realtime/useTerminalListRealtime';
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
import { useContactsRealtime } from '../../lib/realtime/useContactsRealtime';
import { useProjectsRealtime } from '../../lib/realtime/useProjectsRealtime';
import { useRemoteConnectionsRealtime } from '../../lib/realtime/useRemoteConnectionsRealtime';
import { useSessionsRealtime } from '../../lib/realtime/useSessionsRealtime';
import type { ContactItem } from './types';

const CONTACT_CREATE_LIKE_REASONS = new Set(['contact_created', 'contact_upserted']);
const CONTACT_UPDATE_LIKE_REASONS = new Set(['contact_updated']);
const PROJECT_CREATE_LIKE_REASONS = new Set(['project_created']);
const PROJECT_UPDATE_LIKE_REASONS = new Set(['project_updated']);
const REMOTE_CONNECTION_CREATE_LIKE_REASONS = new Set(['remote_connection_created']);
const REMOTE_CONNECTION_UPDATE_LIKE_REASONS = new Set(['remote_connection_updated']);
const SESSION_CREATE_LIKE_REASONS = new Set(['session_created']);
const SESSION_UPDATE_LIKE_REASONS = new Set(['session_updated']);

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
  const contextStoreHook = useOptionalChatStoreContext();
  const storeToUse = store || contextStoreHook || useChatStore;

  const {
    sessions,
    contacts,
    agents,
    currentSession,
    activePanel,
    loadSessions,
    loadContacts: loadContactsAction,
    createContact: createContactAction,
    deleteContact: deleteContactAction,
    markContactsStale,
    removeContactLocally,
    applyRealtimeContactSnapshot,
    refreshContactById,
    createSession,
    selectSession,
    deleteSession,
    updateSession,
    markSessionsStale,
    removeSessionLocally,
    applyRealtimeSessionSnapshot,
    refreshSessionById,
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
    markProjectsStale,
    removeProjectLocally,
    applyRealtimeProjectSnapshot,
    refreshProjectById,
    setActivePanel,
    terminals,
    currentTerminal,
    loadTerminals,
    createTerminal,
    selectTerminal,
    deleteTerminal,
    markTerminalsStale,
    removeTerminalLocally,
    applyRealtimeTerminalSnapshot,
    refreshTerminalById,
    remoteConnections,
    currentRemoteConnection,
    loadRemoteConnections,
    createRemoteConnection,
    updateRemoteConnection,
    selectRemoteConnection,
    deleteRemoteConnection,
    openRemoteSftp,
    markRemoteConnectionsStale,
    removeRemoteConnectionLocally,
    applyRealtimeRemoteConnectionSnapshot,
    refreshRemoteConnectionById,
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
  const { confirm, alert } = useDialogService();

  const remoteForm = useRemoteConnectionForm({
    apiClient,
    remoteConnections,
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
    remoteJumpCertificatePath: remoteForm.remoteJumpCertificatePath,
    onProjectRootChange: setProjectRoot,
    onTerminalRootChange: setTerminalRoot,
    onRemotePrivateKeyPathChange: remoteForm.setRemotePrivateKeyPath,
    onRemoteCertificatePathChange: remoteForm.setRemoteCertificatePath,
    onRemoteJumpPrivateKeyPathChange: remoteForm.setRemoteJumpPrivateKeyPath,
    onRemoteJumpCertificatePathChange: remoteForm.setRemoteJumpCertificatePath,
  });

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
    agents,
    currentSessionId: currentSession?.id || null,
    loadContacts: loadContactsAction,
    createContact: createContactAction,
    ensureSessionForContact: contactSessionState.ensureSessionForContact,
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
    ensureSessionForContact: contactSessionState.ensureSessionForContact,
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
    confirmDialog: confirm,
    alertDialog: alert,
  });

  useSessionListBootstrap({
    loadSessions,
    loadProjects,
    loadAgents,
    loadContacts: loadContactsAction,
    loadTerminals,
    loadRemoteConnections,
    isCollapsed,
    terminalsExpanded: sectionExpansion.terminalsExpanded,
    remoteExpanded: sectionExpansion.remoteExpanded,
  });

  useTerminalListRealtime({
    enabled: true,
    onInvalidate: (payload) => {
      const reason = String(payload.reason || '').trim();
      const terminalId = String(payload.terminal_id || '').trim();
      if (reason === 'deleted' && terminalId) {
        removeTerminalLocally(terminalId);
        return;
      }
      if (payload.terminal) {
        applyRealtimeTerminalSnapshot(payload.terminal);
        return;
      }
      if (terminalId) {
        void refreshTerminalById(terminalId).then((terminal) => {
          if (!terminal && reason === 'created') {
            markTerminalsStale();
            void loadTerminals();
          }
        });
        return;
      }
      markTerminalsStale();
      void loadTerminals();
    },
  });

  useContactsRealtime({
    enabled: true,
    onInvalidate: (payload) => {
      const reason = String(payload.reason || '').trim();
      const contactId = String(payload.contact_id || '').trim();
      if (reason === 'contact_deleted' && contactId) {
        removeContactLocally(contactId);
        return;
      }
      if (payload.contact) {
        applyRealtimeContactSnapshot(payload.contact);
        return;
      }
      if (contactId) {
        void refreshContactById(contactId).then((contact) => {
          if (contact) {
            return;
          }
          if (
            CONTACT_CREATE_LIKE_REASONS.has(reason)
            || CONTACT_UPDATE_LIKE_REASONS.has(reason)
          ) {
            markContactsStale();
            if (CONTACT_CREATE_LIKE_REASONS.has(reason)) {
              void loadContactsAction();
            }
          }
        });
        return;
      }
      markContactsStale();
      void loadContactsAction();
    },
  });

  useProjectsRealtime({
    enabled: true,
    onInvalidate: (payload) => {
      const reason = String(payload.reason || '').trim();
      const projectId = String(payload.project_id || '').trim();
      if (reason === 'project_deleted' && projectId) {
        removeProjectLocally(projectId);
        return;
      }
      if (payload.project) {
        applyRealtimeProjectSnapshot(payload.project);
        return;
      }
      if (projectId) {
        void refreshProjectById(projectId).then((project) => {
          if (project) {
            return;
          }
          markProjectsStale({ projectId });
          if (
            PROJECT_CREATE_LIKE_REASONS.has(reason)
            || PROJECT_UPDATE_LIKE_REASONS.has(reason)
          ) {
            if (PROJECT_CREATE_LIKE_REASONS.has(reason)) {
              void loadProjects();
            }
          }
        });
        return;
      }
      markProjectsStale();
      void loadProjects();
    },
  });

  useRemoteConnectionsRealtime({
    enabled: true,
    onInvalidate: (payload) => {
      const reason = String(payload.reason || '').trim();
      const connectionId = String(payload.connection_id || '').trim();
      if (reason === 'remote_connection_deleted' && connectionId) {
        removeRemoteConnectionLocally(connectionId);
        return;
      }
      if (payload.connection) {
        applyRealtimeRemoteConnectionSnapshot(payload.connection);
        return;
      }
      if (connectionId) {
        void refreshRemoteConnectionById(connectionId).then((connection) => {
          if (connection) {
            return;
          }
          markRemoteConnectionsStale({ connectionId });
          if (
            REMOTE_CONNECTION_CREATE_LIKE_REASONS.has(reason)
            || REMOTE_CONNECTION_UPDATE_LIKE_REASONS.has(reason)
          ) {
            if (REMOTE_CONNECTION_CREATE_LIKE_REASONS.has(reason)) {
              void loadRemoteConnections();
            }
          }
        });
        return;
      }
      markRemoteConnectionsStale();
      void loadRemoteConnections();
    },
  });

  useSessionsRealtime({
    enabled: true,
    onInvalidate: (payload) => {
      const reason = String(payload.reason || '').trim();
      const sessionId = String(payload.session_id || '').trim();
      if (reason === 'session_deleted' && sessionId) {
        removeSessionLocally(sessionId);
        return;
      }
      if (payload.session) {
        applyRealtimeSessionSnapshot(payload.session);
        return;
      }
      if (sessionId) {
        void refreshSessionById(sessionId).then((session) => {
          if (session) {
            return;
          }
          markSessionsStale({ sessionId });
          if (
            SESSION_CREATE_LIKE_REASONS.has(reason)
            || SESSION_UPDATE_LIKE_REASONS.has(reason)
          ) {
            if (SESSION_CREATE_LIKE_REASONS.has(reason)) {
              void loadSessions({ silent: true });
            }
          }
        });
        return;
      }
      markSessionsStale();
      void loadSessions({ silent: true });
    },
  });

  const projectRunState = useProjectRunState({
    apiClient,
    projects,
    terminals,
    loadTerminals,
    handleSelectTerminal: sessionListActions.handleSelectTerminal,
    setActivePanel,
    enabled: activePanel !== 'project',
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
    existingContactAgentIds,
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
