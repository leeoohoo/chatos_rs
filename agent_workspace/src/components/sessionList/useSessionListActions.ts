import { useCallback } from 'react';
import { deriveNameFromPath } from './helpers';
import type { ContactItem } from './types';

interface SessionListActionsParams {
  contacts: ContactItem[];
  currentSession: any;
  terminals: any[];
  currentTerminal: any;
  remoteConnections: any[];
  currentRemoteConnection: any;
  ensureBackingSessionForContactScope: (contact: ContactItem) => Promise<string | null>;
  selectSession: (sessionId: string) => Promise<any>;
  setActivePanel: (panel: any) => void;
  onOpenSessionSummary?: (sessionId: string) => void;
  loadContactsAction: () => Promise<any>;
  loadTerminals: () => Promise<any>;
  loadRemoteConnections: () => Promise<any>;
  setIsRefreshing: (value: boolean) => void;
  setIsRefreshingTerminals: (value: boolean) => void;
  setIsRefreshingRemote: (value: boolean) => void;
  setProjectRoot: (value: string) => void;
  setProjectError: (value: string | null) => void;
  setProjectModalOpen: (value: boolean) => void;
  setTerminalRoot: (value: string) => void;
  setTerminalError: (value: string | null) => void;
  setTerminalModalOpen: (value: boolean) => void;
  setKeyFilePickerOpen: (value: boolean) => void;
  openRemoteModalBase: () => void;
  createProject: (name: string, rootPath: string) => Promise<any>;
  createTerminal: (cwd: string, name: string) => Promise<any>;
  selectProject: (projectId: string) => Promise<any>;
  selectTerminal: (terminalId: string) => Promise<any>;
  selectRemoteConnection: (connectionId: string) => Promise<any>;
  openRemoteSftp: (connectionId: string) => Promise<any>;
  projectRoot: string;
  terminalRoot: string;
}

export const useSessionListActions = ({
  contacts,
  currentSession,
  terminals,
  currentTerminal,
  remoteConnections,
  currentRemoteConnection,
  ensureBackingSessionForContactScope,
  selectSession,
  setActivePanel,
  onOpenSessionSummary,
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
}: SessionListActionsParams) => {
  const handleSelectSession = useCallback(async (sessionId: string): Promise<string | null> => {
    try {
      const contact = contacts.find((item) => item.id === sessionId);
      if (contact) {
        const ensuredSessionId = await ensureBackingSessionForContactScope(contact);
        if (!ensuredSessionId) {
          return null;
        }
        if (currentSession?.id !== ensuredSessionId) {
          await selectSession(ensuredSessionId);
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
  }, [contacts, currentSession?.id, ensureBackingSessionForContactScope, selectSession, setActivePanel]);

  const handleOpenSummary = useCallback((sessionId: string) => {
    void handleSelectSession(sessionId).then((selectedSessionId) => {
      if (selectedSessionId) {
        onOpenSessionSummary?.(selectedSessionId);
      }
    });
  }, [handleSelectSession, onOpenSessionSummary]);

  const handleRefreshSessions = useCallback(async () => {
    setIsRefreshing(true);
    await loadContactsAction();
    setIsRefreshing(false);
  }, [loadContactsAction, setIsRefreshing]);

  const handleRefreshTerminals = useCallback(async () => {
    setIsRefreshingTerminals(true);
    await loadTerminals();
    setIsRefreshingTerminals(false);
  }, [loadTerminals, setIsRefreshingTerminals]);

  const handleRefreshRemote = useCallback(async () => {
    setIsRefreshingRemote(true);
    await loadRemoteConnections();
    setIsRefreshingRemote(false);
  }, [loadRemoteConnections, setIsRefreshingRemote]);

  const openProjectModal = useCallback(() => {
    setProjectRoot('');
    setProjectError(null);
    setProjectModalOpen(true);
  }, [setProjectError, setProjectModalOpen, setProjectRoot]);

  const openTerminalModal = useCallback(() => {
    setTerminalRoot('');
    setTerminalError(null);
    setTerminalModalOpen(true);
  }, [setTerminalError, setTerminalModalOpen, setTerminalRoot]);

  const openRemoteModal = useCallback(() => {
    setKeyFilePickerOpen(false);
    openRemoteModalBase();
  }, [openRemoteModalBase, setKeyFilePickerOpen]);

  const handleCreateProject = useCallback(async () => {
    if (!projectRoot.trim()) {
      setProjectError('请选择项目目录');
      return;
    }
    try {
      const name = deriveNameFromPath(projectRoot, 'Project');
      await createProject(name, projectRoot.trim());
      setProjectModalOpen(false);
    } catch (error) {
      setProjectError(error instanceof Error ? error.message : '创建项目失败');
    }
  }, [createProject, projectRoot, setProjectError, setProjectModalOpen]);

  const handleCreateTerminal = useCallback(async () => {
    if (!terminalRoot.trim()) {
      setTerminalError('请选择终端目录');
      return;
    }
    try {
      const name = deriveNameFromPath(terminalRoot, 'Terminal');
      await createTerminal(terminalRoot.trim(), name);
      setTerminalModalOpen(false);
    } catch (error) {
      setTerminalError(error instanceof Error ? error.message : '创建终端失败');
    }
  }, [createTerminal, setTerminalError, setTerminalModalOpen, terminalRoot]);

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
