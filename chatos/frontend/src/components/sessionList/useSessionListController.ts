// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import type { TaskRunnerAgentAccountResponse } from '../../lib/api/client/types';
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
import { useTerminalUiSetting } from '../../hooks/useTerminalUiSetting';
import type { ChatStore as SessionListStoreHook } from '../../lib/store/createChatStoreWithBackend';
import type {
  LocalConnectorDirectoryEntryResponse,
  LocalConnectorDeviceResponse,
  LocalConnectorWorkspaceResponse,
} from '../../lib/api/client/types';
import type {
  LocalConnectorDirectoryEntryOption,
  LocalConnectorWorkspaceOption,
  ResourceSourceMode,
} from './CreateResourceModals';
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

const valueAsString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const valueAsBoolean = (value: unknown): boolean => (
  typeof value === 'boolean' ? value : false
);

const normalizeLocalConnectorDirectoryPath = (value: unknown): string => {
  const normalized = valueAsString(value).replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
  return normalized && normalized !== '.' ? normalized : '.';
};

const joinLocalConnectorDirectoryPath = (parent: string, name: string): string => {
  const cleanParent = normalizeLocalConnectorDirectoryPath(parent);
  const cleanName = name.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
  return cleanParent === '.' ? cleanName : `${cleanParent}/${cleanName}`;
};

const localConnectorDeviceId = (device: LocalConnectorDeviceResponse): string => (
  valueAsString(device.id)
);

const localConnectorDeviceLabel = (device: LocalConnectorDeviceResponse): string => (
  valueAsString(device.display_name)
  || valueAsString(device.displayName)
  || valueAsString(device.os)
  || localConnectorDeviceId(device)
);

const localConnectorDeviceStatus = (device: LocalConnectorDeviceResponse): string => (
  valueAsString(device.status) || 'registered'
);

const localConnectorWorkspaceDeviceId = (workspace: LocalConnectorWorkspaceResponse): string => (
  valueAsString(workspace.device_id)
  || valueAsString(workspace.deviceId)
);

const localConnectorWorkspaceAlias = (workspace: LocalConnectorWorkspaceResponse): string => (
  valueAsString(workspace.local_path_alias)
  || valueAsString(workspace.localPathAlias)
  || valueAsString(workspace.display_name)
  || valueAsString(workspace.displayName)
  || valueAsString(workspace.id)
);

const localConnectorWorkspaceStatus = (workspace: LocalConnectorWorkspaceResponse): string => (
  valueAsString(workspace.status) || 'active'
);

const localConnectorDirectoryEntryName = (entry: LocalConnectorDirectoryEntryResponse): string => (
  valueAsString(entry.name) || valueAsString(entry.path)
);

const normalizeLocalConnectorDirectoryEntries = (
  entries: LocalConnectorDirectoryEntryResponse[] | undefined,
): LocalConnectorDirectoryEntryOption[] => (
  (Array.isArray(entries) ? entries : [])
    .map((entry) => {
      const name = localConnectorDirectoryEntryName(entry);
      const path = normalizeLocalConnectorDirectoryPath(entry.path);
      const isDir = valueAsBoolean(entry.is_dir) || valueAsBoolean(entry.isDir);
      return {
        name,
        path,
        isDir,
      };
    })
    .filter((entry) => entry.name && entry.path && entry.isDir)
);

const normalizeLocalConnectorWorkspaces = (
  devices: LocalConnectorDeviceResponse[],
  workspaces: LocalConnectorWorkspaceResponse[],
): LocalConnectorWorkspaceOption[] => {
  const deviceLabels = new Map<string, string>();
  const deviceStatuses = new Map<string, string>();
  devices.forEach((device) => {
    const id = localConnectorDeviceId(device);
    if (id) {
      deviceLabels.set(id, localConnectorDeviceLabel(device));
      deviceStatuses.set(id, localConnectorDeviceStatus(device));
    }
  });
  return workspaces
    .map((workspace) => {
      const id = valueAsString(workspace.id);
      const deviceId = localConnectorWorkspaceDeviceId(workspace);
      const alias = localConnectorWorkspaceAlias(workspace);
      const deviceLabel = deviceLabels.get(deviceId) || null;
      const deviceStatus = deviceStatuses.get(deviceId) || null;
      const label = [alias, deviceLabel].filter(Boolean).join(' · ');
      return {
        id,
        deviceId,
        alias,
        label: label || id,
        deviceLabel,
        deviceStatus,
        status: localConnectorWorkspaceStatus(workspace),
      };
    })
    .filter((workspace) => (
      workspace.id
      && workspace.deviceId
      && workspace.deviceStatus === 'online'
      && workspace.status !== 'disabled'
    ));
};

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
    createCloudProject,
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
  const [cloudProjectName, setCloudProjectName] = useState('');
  const [cloudProjectGitUrl, setCloudProjectGitUrl] = useState('');
  const [cloudProjectZipFile, setCloudProjectZipFile] = useState<File | null>(null);
  const [projectError, setProjectError] = useState<string | null>(null);
  const [projectSourceMode, setProjectSourceMode] = useState<ResourceSourceMode>('server');

  const [terminalModalOpen, setTerminalModalOpen] = useState(false);
  const [terminalError, setTerminalError] = useState<string | null>(null);
  const [terminalExecuting, setTerminalExecuting] = useState(false);
  const [localConnectorWorkspaces, setLocalConnectorWorkspaces] = useState<LocalConnectorWorkspaceOption[]>([]);
  const [localConnectorLoading, setLocalConnectorLoading] = useState(false);
  const [localConnectorError, setLocalConnectorError] = useState<string | null>(null);
  const [selectedLocalConnectorWorkspaceId, setSelectedLocalConnectorWorkspaceId] = useState('');
  const [localConnectorDirectoryPath, setLocalConnectorDirectoryPath] = useState('.');
  const [localConnectorDirectoryParent, setLocalConnectorDirectoryParent] = useState<string | null>(null);
  const [localConnectorDirectoryEntries, setLocalConnectorDirectoryEntries] = useState<LocalConnectorDirectoryEntryOption[]>([]);
  const [localConnectorDirectoryLoading, setLocalConnectorDirectoryLoading] = useState(false);
  const [localConnectorDirectoryError, setLocalConnectorDirectoryError] = useState<string | null>(null);
  const [selectedLocalConnectorDirectoryPath, setSelectedLocalConnectorDirectoryPath] = useState('');
  const [taskRunnerContactId, setTaskRunnerContactId] = useState<string | null>(null);
  const [taskRunnerAgentAccounts, setTaskRunnerAgentAccounts] = useState<TaskRunnerAgentAccountResponse[]>([]);
  const [taskRunnerAgentAccountsLoading, setTaskRunnerAgentAccountsLoading] = useState(false);
  const [taskRunnerError, setTaskRunnerError] = useState<string | null>(null);
  const [taskRunnerSaving, setTaskRunnerSaving] = useState(false);

  const apiClient = useApiClient();
  const { confirm, alert } = useDialogService();

  const loadLocalConnectorDirectoryForWorkspace = useCallback(async (
    workspace: LocalConnectorWorkspaceOption | null,
    path = '.',
  ) => {
    if (!workspace) {
      setLocalConnectorDirectoryPath('.');
      setLocalConnectorDirectoryParent(null);
      setLocalConnectorDirectoryEntries([]);
      setSelectedLocalConnectorDirectoryPath('');
      return;
    }
    const normalizedPath = normalizeLocalConnectorDirectoryPath(path);
    setLocalConnectorDirectoryLoading(true);
    setLocalConnectorDirectoryError(null);
    try {
      const result = await apiClient.listLocalConnectorDirectory({
        device_id: workspace.deviceId,
        workspace_id: workspace.id,
        path: normalizedPath,
      });
      const nextPath = normalizeLocalConnectorDirectoryPath(result.path);
      const parent = result.parent ? normalizeLocalConnectorDirectoryPath(result.parent) : null;
      setLocalConnectorDirectoryPath(nextPath);
      setLocalConnectorDirectoryParent(parent && parent !== nextPath ? parent : null);
      setLocalConnectorDirectoryEntries(normalizeLocalConnectorDirectoryEntries(result.entries));
      setSelectedLocalConnectorDirectoryPath(nextPath === '.' ? '' : nextPath);
    } catch (error) {
      setLocalConnectorDirectoryEntries([]);
      setLocalConnectorDirectoryError(error instanceof Error ? error.message : t('sessionList.resource.localConnectorDirectoryLoadFailed'));
    } finally {
      setLocalConnectorDirectoryLoading(false);
    }
  }, [apiClient, t]);

  const refreshLocalConnectorWorkspaces = useCallback(async () => {
    setLocalConnectorLoading(true);
    setLocalConnectorError(null);
    try {
      const [devices, workspaces] = await Promise.all([
        apiClient.listLocalConnectorDevices(),
        apiClient.listLocalConnectorWorkspaces(),
      ]);
      const normalized = normalizeLocalConnectorWorkspaces(
        Array.isArray(devices) ? devices : [],
        Array.isArray(workspaces) ? workspaces : [],
      );
      setLocalConnectorWorkspaces(normalized);
      const nextWorkspace = normalized.find((workspace) => workspace.id === selectedLocalConnectorWorkspaceId)
        || normalized[0]
        || null;
      setSelectedLocalConnectorWorkspaceId(nextWorkspace?.id || '');
      await loadLocalConnectorDirectoryForWorkspace(nextWorkspace, '.');
    } catch (error) {
      setLocalConnectorWorkspaces([]);
      setLocalConnectorDirectoryPath('.');
      setLocalConnectorDirectoryParent(null);
      setLocalConnectorDirectoryEntries([]);
      setSelectedLocalConnectorDirectoryPath('');
      setLocalConnectorError(error instanceof Error ? error.message : t('sessionList.resource.localConnectorLoadFailed'));
    } finally {
      setLocalConnectorLoading(false);
    }
  }, [apiClient, loadLocalConnectorDirectoryForWorkspace, selectedLocalConnectorWorkspaceId, t]);

  const handleSelectedLocalConnectorWorkspaceChange = useCallback((workspaceId: string) => {
    setSelectedLocalConnectorWorkspaceId(workspaceId);
    setSelectedLocalConnectorDirectoryPath('');
    const workspace = localConnectorWorkspaces.find((item) => item.id === workspaceId) || null;
    void loadLocalConnectorDirectoryForWorkspace(workspace, '.');
  }, [loadLocalConnectorDirectoryForWorkspace, localConnectorWorkspaces]);

  const browseLocalConnectorDirectory = useCallback((path: string) => {
    const workspace = localConnectorWorkspaces.find((item) => item.id === selectedLocalConnectorWorkspaceId) || null;
    void loadLocalConnectorDirectoryForWorkspace(workspace, path);
  }, [loadLocalConnectorDirectoryForWorkspace, localConnectorWorkspaces, selectedLocalConnectorWorkspaceId]);

  const createLocalConnectorDirectory = useCallback(async (name: string) => {
    const workspace = localConnectorWorkspaces.find((item) => item.id === selectedLocalConnectorWorkspaceId) || null;
    if (!workspace) {
      setLocalConnectorDirectoryError(t('sessionList.resource.error.selectLocalConnectorWorkspace'));
      return;
    }
    const targetPath = joinLocalConnectorDirectoryPath(localConnectorDirectoryPath, name);
    setLocalConnectorDirectoryLoading(true);
    setLocalConnectorDirectoryError(null);
    try {
      const result = await apiClient.createLocalConnectorDirectory({
        device_id: workspace.deviceId,
        workspace_id: workspace.id,
        path: targetPath,
      });
      const nextPath = normalizeLocalConnectorDirectoryPath(result.path || targetPath);
      await loadLocalConnectorDirectoryForWorkspace(workspace, nextPath);
    } catch (error) {
      setLocalConnectorDirectoryError(error instanceof Error ? error.message : t('sessionList.resource.localConnectorDirectoryCreateFailed'));
    } finally {
      setLocalConnectorDirectoryLoading(false);
    }
  }, [
    apiClient,
    loadLocalConnectorDirectoryForWorkspace,
    localConnectorDirectoryPath,
    localConnectorWorkspaces,
    selectedLocalConnectorWorkspaceId,
    t,
  ]);

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
    remotePrivateKeyPath: remoteForm.remotePrivateKeyPath,
    remoteCertificatePath: remoteForm.remoteCertificatePath,
    remoteJumpPrivateKeyPath: remoteForm.remoteJumpPrivateKeyPath,
    remoteJumpCertificatePath: remoteForm.remoteJumpCertificatePath,
    onProjectRootChange: setProjectRoot,
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

  const openTaskRunnerConfig = async (displaySessionId: string) => {
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
    setTaskRunnerAgentAccountsLoading(true);
    try {
      const items = await apiClient.listTaskRunnerAgentAccounts();
      setTaskRunnerAgentAccounts(Array.isArray(items) ? items : []);
    } catch (error) {
      setTaskRunnerAgentAccounts([]);
      setTaskRunnerError(error instanceof Error ? error.message : t('taskRunnerConfig.loadAgentAccountsFailed'));
    } finally {
      setTaskRunnerAgentAccountsLoading(false);
    }
  };

  const closeTaskRunnerConfig = () => {
    setTaskRunnerContactId(null);
    setTaskRunnerAgentAccounts([]);
    setTaskRunnerAgentAccountsLoading(false);
    setTaskRunnerError(null);
  };

  const saveTaskRunnerConfig = async (values: {
    enabled: boolean;
    agentAccountId: string;
  }) => {
    if (!taskRunnerContact) {
      return;
    }
    const agentAccountId = values.agentAccountId.trim();
    if (values.enabled && !agentAccountId) {
      setTaskRunnerError(t('taskRunnerConfig.agentAccountMissing'));
      return;
    }
    setTaskRunnerSaving(true);
    setTaskRunnerError(null);
    try {
      await updateContactTaskRunnerConfig(taskRunnerContact.id, {
        ...values,
        agentAccountId,
        username: '',
        clearPassword: true,
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
    setCloudProjectName,
    setCloudProjectGitUrl,
    setCloudProjectZipFile,
    setProjectError,
    setProjectModalOpen,
    setProjectSourceMode,
    setTerminalError,
    setTerminalModalOpen,
    setKeyFilePickerOpen: localFsPickers.setKeyFilePickerOpen,
    openRemoteModalBase: remoteForm.openRemoteModal,
    createCloudProject,
    createTerminal,
    selectProject,
    selectTerminal,
    loadProjects,
    apiClient,
    projectSourceMode,
    localConnectorWorkspaces,
    selectedLocalConnectorWorkspaceId,
    selectedLocalConnectorDirectoryPath,
    setTerminalExecuting,
    selectRemoteConnection,
    openRemoteSftp,
    cloudProjectName,
    cloudProjectGitUrl,
    cloudProjectZipFile,
  });

  const sectionExpansion = useSectionExpansion({
    onFocusTerminal: sessionListActions.focusTerminalPanel,
    onFocusRemote: sessionListActions.focusRemotePanel,
  });

  const {
    terminalUiEnabled,
    terminalUiResolved,
  } = useTerminalUiSetting();
  const terminalVisibility = useMemo(() => ({
    terminalUiEnabled,
    terminalUiResolved,
    showTerminalSection: terminalUiResolved && terminalUiEnabled,
  }), [terminalUiEnabled, terminalUiResolved]);

  useEffect(() => {
    if (!terminalVisibility.terminalUiResolved || terminalVisibility.terminalUiEnabled) {
      return;
    }
    if (activePanel === 'terminal') {
      setActivePanel(currentProject ? 'project' : 'chat');
    }
  }, [
    activePanel,
    currentProject,
    setActivePanel,
    terminalVisibility.terminalUiEnabled,
    terminalVisibility.terminalUiResolved,
  ]);

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
    terminalsEnabled: terminalVisibility.showTerminalSection,
    terminalsExpanded: sectionExpansion.terminalsExpanded,
    remoteExpanded: sectionExpansion.remoteExpanded,
  });

  useTerminalListRealtime({
    enabled: terminalVisibility.showTerminalSection,
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
    localConnectorWorkspaces,
    localConnectorLoading,
    localConnectorError,
    localConnectorDirectoryPath,
    localConnectorDirectoryParent,
    localConnectorDirectoryEntries,
    localConnectorDirectoryLoading,
    localConnectorDirectoryError,
    selectedLocalConnectorDirectoryPath,
    refreshLocalConnectorWorkspaces,
    browseLocalConnectorDirectory,
    createLocalConnectorDirectory,
    setSelectedLocalConnectorDirectoryPath,
    openTaskRunnerConfig,
    projectError,
    projectModalOpen,
    projectRoot,
    cloudProjectName,
    cloudProjectGitUrl,
    cloudProjectZipFile,
    projectSourceMode,
    projects,
    remoteConnections,
    remoteForm,
    sectionExpansion,
    sessionChatState,
    sessionListActions,
    sessions,
    setProjectModalOpen,
    setProjectRoot,
    setCloudProjectName,
    setCloudProjectGitUrl,
    setCloudProjectZipFile,
    setProjectSourceMode,
    setTerminalModalOpen,
    setSelectedLocalConnectorWorkspaceId: handleSelectedLocalConnectorWorkspaceChange,
    taskRunnerContact,
    taskRunnerAgentAccounts,
    taskRunnerAgentAccountsLoading,
    taskRunnerError,
    taskRunnerSaving,
    closeTaskRunnerConfig,
    saveTaskRunnerConfig,
    terminals,
    terminalVisibility,
    terminalError,
    terminalModalOpen,
    terminalExecuting,
    selectedLocalConnectorWorkspaceId,
  };
};
