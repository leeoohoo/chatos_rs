import { useMemo, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import {
  useOptionalChatStoreContext,
} from '../../lib/store/ChatStoreContext';
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
import { useSessionListDeleteActions } from './useSessionListDeleteActions';
import { useSessionListActions } from './useSessionListActions';
import { useSessionListStoreState } from './useSessionListStoreState';
import { useRemoteConnectionForm } from './useRemoteConnectionForm';
import { useContactsRealtime } from '../../lib/realtime/useContactsRealtime';
import { useProjectsRealtime } from '../../lib/realtime/useProjectsRealtime';
import { useRemoteConnectionsRealtime } from '../../lib/realtime/useRemoteConnectionsRealtime';
import { useSessionsRealtime } from '../../lib/realtime/useSessionsRealtime';
import type { ChatStore as SessionListStoreHook } from '../../lib/store/createChatStoreWithBackend';
import type { ContactItem } from './types';

const CONTACT_CREATE_LIKE_REASONS = new Set(['contact_created', 'contact_upserted']);
const CONTACT_UPDATE_LIKE_REASONS = new Set(['contact_updated']);
const PROJECT_CREATE_LIKE_REASONS = new Set(['project_created']);
const PROJECT_UPDATE_LIKE_REASONS = new Set(['project_updated']);
const REMOTE_CONNECTION_CREATE_LIKE_REASONS = new Set(['remote_connection_created']);
const REMOTE_CONNECTION_UPDATE_LIKE_REASONS = new Set(['remote_connection_updated']);
const SESSION_CREATE_LIKE_REASONS = new Set(['session_created']);
const SESSION_UPDATE_LIKE_REASONS = new Set(['session_updated']);
const TERMINAL_CREATE_LIKE_REASONS = new Set(['created', 'ensured_running']);
const TERMINAL_UPDATE_LIKE_REASONS = new Set(['updated']);
const TERMINAL_REFRESH_LIKE_REASONS = new Set(['closed']);

interface SessionListControllerParams {
  store?: SessionListStoreHook;
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
  const { t } = useI18n();
  const contextStoreHook = useOptionalChatStoreContext();
  const storeToUse = store || contextStoreHook;
  if (!storeToUse) {
    throw new Error('useSessionListController requires ChatStoreProvider or an explicit store');
  }

  const {
    sessions,
    contacts,
    agents,
    currentSession,
    activePanel,
    loadSessions,
    loadContacts: loadContactsAction,
    createContact: createContactAction,
    updateContactTaskRunnerConfig,
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
  const [taskRunnerContactId, setTaskRunnerContactId] = useState<string | null>(null);
  const [taskRunnerError, setTaskRunnerError] = useState<string | null>(null);
  const [taskRunnerSaving, setTaskRunnerSaving] = useState(false);

  const apiClient = useApiClient();
  const { confirm, alert } = useDialogService();

  const remoteForm = useRemoteConnectionForm({
    apiClient,
    t,
    remoteConnections,
    createRemoteConnection,
    updateRemoteConnection,
  });

  const localFsPickers = useLocalFsPickers({
    apiClient,
    t,
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

  const taskRunnerContact = useMemo(
    () => (contacts || []).find((item: ContactItem) => item.id === taskRunnerContactId) || null,
    [contacts, taskRunnerContactId],
  );

  const contactSessionState = useContactSessionListState({
    t,
    contacts,
    sessions: sessions || [],
    currentSession,
    activePanel,
    activeSummarySessionId,
    createSession,
    apiClient,
  });

  const contactSessionCreator = useContactSessionCreator({
    t,
    agents,
    currentSessionId: currentSession?.id || null,
    loadContacts: loadContactsAction,
    createContact: createContactAction,
    ensureSessionForContact: contactSessionState.ensureSessionForContact,
    updateSession,
    selectSession,
  });

  const inlineActionMenus = useInlineActionMenus();

  const openTaskRunnerConfig = (displaySessionId: string) => {
    const contactId = displaySessionId.startsWith('contact-placeholder:')
      ? displaySessionId.replace('contact-placeholder:', '').trim()
      : '';
    const contact = (contacts || []).find((item: ContactItem) => item.id === contactId);
    if (!contact) {
      setTaskRunnerError(t('taskRunnerConfig.contactMissing'));
      return;
    }
    setTaskRunnerContactId(contact.id);
    setTaskRunnerError(null);
  };

  const closeTaskRunnerConfig = () => {
    setTaskRunnerContactId(null);
    setTaskRunnerError(null);
  };

  const saveTaskRunnerConfig = async (values: {
    enabled: boolean;
    baseUrl: string;
    username: string;
    password?: string;
    clearPassword?: boolean;
  }) => {
    if (!taskRunnerContact) {
      return;
    }
    const baseUrl = values.baseUrl.trim();
    const username = values.username.trim();
    if (values.enabled && (!baseUrl || !username)) {
      setTaskRunnerError(t('taskRunnerConfig.missingEndpoint'));
      return;
    }
    if (
      values.enabled
      && !values.password?.trim()
      && !taskRunnerContact.taskRunner?.hasPassword
      && !values.clearPassword
    ) {
      setTaskRunnerError(t('taskRunnerConfig.missingPassword'));
      return;
    }
    setTaskRunnerSaving(true);
    setTaskRunnerError(null);
    try {
      await updateContactTaskRunnerConfig(taskRunnerContact.id, {
        ...values,
        baseUrl,
        username,
      });
      closeTaskRunnerConfig();
    } catch (error) {
      setTaskRunnerError(error instanceof Error ? error.message : t('taskRunnerConfig.saveFailed'));
    } finally {
      setTaskRunnerSaving(false);
    }
  };

  const sessionListActions = useSessionListActions({
    t,
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
    t,
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
        if (currentTerminal?.id === terminalId) {
          setActivePanel('chat');
        }
        removeTerminalLocally(terminalId);
        return;
      }
      if (payload.terminal && (TERMINAL_CREATE_LIKE_REASONS.has(reason) || TERMINAL_UPDATE_LIKE_REASONS.has(reason))) {
        applyRealtimeTerminalSnapshot(payload.terminal);
        return;
      }
      if (terminalId && (TERMINAL_REFRESH_LIKE_REASONS.has(reason) || !payload.terminal)) {
        void refreshTerminalById(terminalId).then((terminal) => {
          if (!terminal && reason === 'created') {
            markTerminalsStale();
            void loadTerminals();
          }
        });
        return;
      }
      if (terminalId) {
        markTerminalsStale(undefined);
        void loadTerminals();
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
    openTaskRunnerConfig,
    projectError,
    projectModalOpen,
    projectRoot,
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
    taskRunnerContact,
    taskRunnerError,
    taskRunnerSaving,
    closeTaskRunnerConfig,
    saveTaskRunnerConfig,
    terminals,
    terminalError,
    terminalModalOpen,
    terminalRoot,
  };
};
