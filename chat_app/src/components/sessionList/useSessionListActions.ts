// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type ApiClient from '../../lib/api/client';
import { getUserVisiblePath } from '../../lib/domain/filesystem';
import { deriveNameFromPath, translateSessionListMessage } from './helpers';
import type { ChatState, SessionSelectOptions } from '../../lib/store/types';
import type { Project, RemoteConnection, Session, Terminal } from '../../types';
import type {
  LocalConnectorWorkspaceOption,
  ResourceSourceMode,
} from './CreateResourceModals';
import type { ContactItem } from './types';

type ActivePanel = ChatState['activePanel'];

const encodeLocalConnectorRelativePath = (path: string): string => (
  path
    .split('/')
    .map((part) => encodeURIComponent(part))
    .join('/')
);

const localConnectorRootPath = (
  workspace: LocalConnectorWorkspaceOption,
  relativePath: string,
): string => {
  const base = `local://connector/${workspace.deviceId}/${workspace.id}`;
  const normalized = relativePath.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
  return normalized ? `${base}/${encodeLocalConnectorRelativePath(normalized)}` : base;
};

interface SessionListActionsParams {
  apiClient: ApiClient;
  t?: TranslateFn;
  contacts: ContactItem[];
  currentSession: Session | null;
  terminals: Terminal[];
  currentTerminal: Terminal | null;
  remoteConnections: RemoteConnection[];
  currentRemoteConnection: RemoteConnection | null;
  ensureSessionForContact: (contact: ContactItem) => Promise<string | null>;
  selectSession: (sessionId: string, options?: SessionSelectOptions) => Promise<void>;
  setActivePanel: (panel: ActivePanel) => void;
  onOpenSessionSummary?: (sessionId: string) => void;
  onOpenSessionRuntimeContext?: (sessionId: string) => void;
  loadContactsAction: (options?: { force?: boolean }) => Promise<unknown>;
  loadTerminals: (options?: { force?: boolean }) => Promise<unknown>;
  loadRemoteConnections: (options?: { force?: boolean }) => Promise<unknown>;
  setIsRefreshing: (value: boolean) => void;
  setIsRefreshingTerminals: (value: boolean) => void;
  setIsRefreshingRemote: (value: boolean) => void;
  setProjectRoot: (value: string) => void;
  setCloudProjectName: (value: string) => void;
  setCloudProjectGitUrl: (value: string) => void;
  setCloudProjectZipFile: (value: File | null) => void;
  setProjectError: (value: string | null) => void;
  setProjectModalOpen: (value: boolean) => void;
  setProjectSourceMode: (value: ResourceSourceMode) => void;
  setTerminalRoot: (value: string) => void;
  setTerminalError: (value: string | null) => void;
  setTerminalModalOpen: (value: boolean) => void;
  setTerminalSourceMode: (value: ResourceSourceMode) => void;
  setTerminalCommand: (value: string) => void;
  setTerminalArgs: (value: string) => void;
  setTerminalOutput: (value: string | null) => void;
  setTerminalExecuting: (value: boolean) => void;
  setKeyFilePickerOpen: (value: boolean) => void;
  openRemoteModalBase: () => void;
  createCloudProject: (input: {
    name: string;
    gitUrl?: string;
    zipFile?: File | null;
    description?: string;
  }) => Promise<Project>;
  createTerminal: (cwd: string, name: string) => Promise<Terminal>;
  selectProject: (projectId: string) => Promise<void>;
  selectTerminal: (terminalId: string) => Promise<void>;
  loadProjects: (options?: { force?: boolean }) => Promise<unknown>;
  projectSourceMode: ResourceSourceMode;
  terminalSourceMode: ResourceSourceMode;
  localConnectorWorkspaces: LocalConnectorWorkspaceOption[];
  selectedLocalConnectorWorkspaceId: string;
  selectedLocalConnectorDirectoryPath?: string;
  terminalCommand: string;
  terminalArgs: string;
  selectRemoteConnection: (connectionId: string) => Promise<void>;
  openRemoteSftp: (connectionId: string) => Promise<void>;
  cloudProjectName: string;
  cloudProjectGitUrl: string;
  cloudProjectZipFile: File | null;
  terminalRoot: string;
}

export const useSessionListActions = ({
  apiClient,
  t,
  contacts,
  currentSession,
  terminals,
  currentTerminal,
  remoteConnections,
  currentRemoteConnection,
  ensureSessionForContact,
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
  setTerminalRoot,
  setTerminalError,
  setTerminalModalOpen,
  setTerminalSourceMode,
  setTerminalCommand,
  setTerminalArgs,
  setTerminalOutput,
  setTerminalExecuting,
  setKeyFilePickerOpen,
  openRemoteModalBase,
  createCloudProject,
  createTerminal,
  selectProject,
  selectTerminal,
  loadProjects,
  projectSourceMode,
  terminalSourceMode,
  localConnectorWorkspaces,
  selectedLocalConnectorWorkspaceId,
  selectedLocalConnectorDirectoryPath = '',
  selectRemoteConnection,
  openRemoteSftp,
  cloudProjectName,
  cloudProjectGitUrl,
  cloudProjectZipFile,
  terminalRoot,
}: SessionListActionsParams) => {
  const localConnectorRelativePath = selectedLocalConnectorDirectoryPath.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');

  const handleSelectSession = useCallback(async (sessionId: string): Promise<string | null> => {
    try {
      if (sessionId.startsWith('contact-placeholder:')) {
        const contactId = sessionId.replace('contact-placeholder:', '').trim();
        const contact = contacts.find((item) => item.id === contactId);
        if (!contact) {
          return null;
        }
        const ensuredSessionId = await ensureSessionForContact(contact);
        if (!ensuredSessionId) {
          return null;
        }
        if (currentSession?.id !== ensuredSessionId) {
          await selectSession(ensuredSessionId, {
            skipBackgroundSync: true,
          });
        } else {
          setActivePanel('chat');
        }
        return ensuredSessionId;
      }
      if (currentSession?.id !== sessionId) {
        await selectSession(sessionId);
      } else {
        setActivePanel('chat');
      }
      return sessionId;
    } catch (error) {
      console.error('Failed to select session:', error);
      return null;
    }
  }, [contacts, currentSession?.id, ensureSessionForContact, selectSession, setActivePanel]);

  const handleOpenSummary = useCallback((sessionId: string) => {
    void handleSelectSession(sessionId).then((selectedSessionId) => {
      if (selectedSessionId) {
        onOpenSessionSummary?.(selectedSessionId);
      }
    });
  }, [handleSelectSession, onOpenSessionSummary]);

  const handleOpenRuntimeContext = useCallback((sessionId: string) => {
    void handleSelectSession(sessionId).then((selectedSessionId) => {
      if (selectedSessionId) {
        onOpenSessionRuntimeContext?.(selectedSessionId);
      }
    });
  }, [handleSelectSession, onOpenSessionRuntimeContext]);

  const handleRefreshSessions = useCallback(async () => {
    setIsRefreshing(true);
    await loadContactsAction({ force: true });
    setIsRefreshing(false);
  }, [loadContactsAction, setIsRefreshing]);

  const handleRefreshTerminals = useCallback(async () => {
    setIsRefreshingTerminals(true);
    await loadTerminals({ force: true });
    setIsRefreshingTerminals(false);
  }, [loadTerminals, setIsRefreshingTerminals]);

  const handleRefreshRemote = useCallback(async () => {
    setIsRefreshingRemote(true);
    await loadRemoteConnections({ force: true });
    setIsRefreshingRemote(false);
  }, [loadRemoteConnections, setIsRefreshingRemote]);

  const openProjectModal = useCallback(() => {
    setProjectRoot('');
    setCloudProjectName('');
    setCloudProjectGitUrl('');
    setCloudProjectZipFile(null);
    setProjectError(null);
    setProjectSourceMode('server');
    setProjectModalOpen(true);
  }, [
    setCloudProjectGitUrl,
    setCloudProjectName,
    setCloudProjectZipFile,
    setProjectError,
    setProjectModalOpen,
    setProjectRoot,
    setProjectSourceMode,
  ]);

  const openTerminalModal = useCallback(() => {
    setTerminalRoot('');
    setTerminalError(null);
    setTerminalSourceMode('server');
    setTerminalCommand('pwd');
    setTerminalArgs('');
    setTerminalOutput(null);
    setTerminalModalOpen(true);
  }, [
    setTerminalArgs,
    setTerminalCommand,
    setTerminalError,
    setTerminalModalOpen,
    setTerminalOutput,
    setTerminalRoot,
    setTerminalSourceMode,
  ]);

  const openRemoteModal = useCallback(() => {
    setKeyFilePickerOpen(false);
    openRemoteModalBase();
  }, [openRemoteModalBase, setKeyFilePickerOpen]);

  const handleCreateProject = useCallback(async () => {
    if (projectSourceMode === 'local_connector') {
      const workspace = localConnectorWorkspaces.find((item) => item.id === selectedLocalConnectorWorkspaceId);
      if (!workspace) {
        setProjectError(translateSessionListMessage(t, 'sessionList.resource.error.selectLocalConnectorWorkspace'));
        return;
      }
      try {
        const name = deriveNameFromPath(localConnectorRelativePath || workspace.alias, 'Project');
        const created = await apiClient.createLocalConnectorProject({
          name,
          device_id: workspace.deviceId,
          workspace_id: workspace.id,
          relative_path: localConnectorRelativePath || undefined,
        });
        await loadProjects({ force: true });
        if (created.id) {
          await selectProject(created.id);
        }
        setProjectModalOpen(false);
      } catch (error) {
        setProjectError(error instanceof Error ? error.message : translateSessionListMessage(t, 'sessionList.resource.error.createProjectFailed'));
      }
      return;
    }
    const normalizedName = cloudProjectName.trim();
    const normalizedGitUrl = cloudProjectGitUrl.trim();
    if (!normalizedName) {
      setProjectError(translateSessionListMessage(t, 'sessionList.resource.error.enterProjectName'));
      return;
    }
    if (normalizedGitUrl && cloudProjectZipFile) {
      setProjectError(translateSessionListMessage(t, 'sessionList.resource.error.gitUrlOrZipOnly'));
      return;
    }
    try {
      await createCloudProject({
        name: normalizedName,
        gitUrl: normalizedGitUrl || undefined,
        zipFile: cloudProjectZipFile,
      });
      setProjectModalOpen(false);
    } catch (error) {
      setProjectError(error instanceof Error ? error.message : translateSessionListMessage(t, 'sessionList.resource.error.createProjectFailed'));
    }
  }, [
    apiClient,
    cloudProjectGitUrl,
    cloudProjectName,
    cloudProjectZipFile,
    createCloudProject,
    loadProjects,
    localConnectorWorkspaces,
    localConnectorRelativePath,
    projectSourceMode,
    selectedLocalConnectorWorkspaceId,
    selectProject,
    setProjectError,
    setProjectModalOpen,
    t,
  ]);

  const handleCreateTerminal = useCallback(async () => {
    if (terminalSourceMode === 'local_connector') {
      const workspace = localConnectorWorkspaces.find((item) => item.id === selectedLocalConnectorWorkspaceId);
      if (!workspace) {
        setTerminalError(translateSessionListMessage(t, 'sessionList.resource.error.selectLocalConnectorWorkspace'));
        return;
      }
      setTerminalExecuting(true);
      setTerminalError(null);
      try {
        const root = localConnectorRootPath(workspace, localConnectorRelativePath);
        const name = deriveNameFromPath(localConnectorRelativePath || workspace.alias, 'Terminal');
        await createTerminal(root, name);
        setTerminalModalOpen(false);
        setTerminalOutput(null);
      } catch (error) {
        setTerminalError(error instanceof Error ? error.message : translateSessionListMessage(t, 'sessionList.resource.error.createTerminalFailed'));
      } finally {
        setTerminalExecuting(false);
      }
      return;
    }
    if (!terminalRoot.trim()) {
      setTerminalError(translateSessionListMessage(t, 'sessionList.resource.error.selectTerminalDirectory'));
      return;
    }
    try {
      const name = deriveNameFromPath(getUserVisiblePath(terminalRoot), 'Terminal');
      await createTerminal(terminalRoot.trim(), name);
      setTerminalModalOpen(false);
    } catch (error) {
      setTerminalError(error instanceof Error ? error.message : translateSessionListMessage(t, 'sessionList.resource.error.createTerminalFailed'));
    }
  }, [
    createTerminal,
    localConnectorRelativePath,
    localConnectorWorkspaces,
    selectedLocalConnectorWorkspaceId,
    setTerminalError,
    setTerminalExecuting,
    setTerminalModalOpen,
    setTerminalOutput,
    t,
    terminalRoot,
    terminalSourceMode,
  ]);

  const handleSelectProject = useCallback(async (projectId: string) => {
    try {
      await selectProject(projectId);
    } catch (error) {
      console.error('Failed to select project:', error);
    }
  }, [selectProject]);

  const handleSelectTerminal = useCallback(async (terminalId: string) => {
    try {
      await selectTerminal(terminalId);
    } catch (error) {
      console.error('Failed to select terminal:', error);
    }
  }, [selectTerminal]);

  const handleSelectRemoteConnection = useCallback(async (connectionId: string) => {
    try {
      await selectRemoteConnection(connectionId);
    } catch (error) {
      console.error('Failed to select remote connection:', error);
    }
  }, [selectRemoteConnection]);

  const handleOpenRemoteSftp = useCallback(async (connectionId: string) => {
    try {
      await openRemoteSftp(connectionId);
    } catch (error) {
      console.error('Failed to open remote sftp:', error);
    }
  }, [openRemoteSftp]);

  const focusTerminalPanel = useCallback(() => {
    const targetTerminalId = currentTerminal?.id || terminals[0]?.id || null;
    if (targetTerminalId) {
      void handleSelectTerminal(targetTerminalId);
      return;
    }
    setActivePanel('terminal');
  }, [currentTerminal?.id, handleSelectTerminal, setActivePanel, terminals]);

  const focusRemotePanel = useCallback(() => {
    const targetConnectionId = currentRemoteConnection?.id || remoteConnections[0]?.id || null;
    if (targetConnectionId) {
      void handleSelectRemoteConnection(targetConnectionId);
      return;
    }
    setActivePanel('remote_terminal');
  }, [currentRemoteConnection?.id, handleSelectRemoteConnection, remoteConnections, setActivePanel]);

  return {
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
  };
};
