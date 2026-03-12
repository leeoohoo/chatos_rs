import React, { useCallback, useState, useEffect, useRef } from 'react';
import { shallow } from 'zustand/shallow';
import { useChatStoreContext, useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import { apiClient as globalApiClient } from '../lib/api/client';
import type { Session, Project, FsEntry, Terminal, RemoteConnection } from '../types';
import ConfirmDialog from './ui/ConfirmDialog';
import { useConfirmDialog } from '../hooks/useConfirmDialog';
import { cn } from '../lib/utils';
import { resolveRemoteConnectionErrorFeedback } from '../lib/api/remoteConnectionErrors';
import { DirPickerDialog, KeyFilePickerDialog } from './sessionList/Pickers';
import { RemoteConnectionModal } from './sessionList/RemoteConnectionModal';
import { CreateProjectModal, CreateTerminalModal } from './sessionList/CreateResourceModals';
import {
  ProjectSection,
  RemoteSection,
  SessionSection,
  TerminalSection,
} from './sessionList/Sections';
import {
  deriveNameFromPath,
  deriveParentPath,
  formatTimeAgo,
  getKeyFilePickerTitle,
  getSessionStatus,
  normalizeFsEntry,
} from './sessionList/helpers';
import { useRemoteConnectionForm } from './sessionList/useRemoteConnectionForm';
import { useSessionSummaryStatus } from './sessionList/useSessionSummaryStatus';
import type {
  DirPickerTarget,
  KeyFilePickerTarget,
} from './sessionList/helpers';

interface SessionListProps {
  isOpen?: boolean;
  onClose?: () => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
  className?: string;
  store?: typeof useChatStore;
  onOpenSummary?: (sessionId: string) => void;
  onSelectSession?: (sessionId: string) => void;
  summaryOpenSessionId?: string | null;
}

export const SessionList: React.FC<SessionListProps> = (props) => {
  const {
    isOpen = true,
    collapsed,
    className,
    store,
    onOpenSummary,
    onSelectSession,
    summaryOpenSessionId = null,
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
    currentSession,
    createSession,
    selectSession,
    deleteSession,
    updateSession,
    loadSessions,
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
  } = storeToUse((state) => ({
    sessions: state.sessions,
    currentSession: state.currentSession,
    createSession: state.createSession,
    selectSession: state.selectSession,
    deleteSession: state.deleteSession,
    updateSession: state.updateSession,
    loadSessions: state.loadSessions,
    sessionChatState: state.sessionChatState,
    taskReviewPanelsBySession: state.taskReviewPanelsBySession,
    uiPromptPanelsBySession: state.uiPromptPanelsBySession,
    projects: state.projects,
    currentProject: state.currentProject,
    loadProjects: state.loadProjects,
    createProject: state.createProject,
    selectProject: state.selectProject,
    deleteProject: state.deleteProject,
    setActivePanel: state.setActivePanel,
    terminals: state.terminals,
    currentTerminal: state.currentTerminal,
    loadTerminals: state.loadTerminals,
    createTerminal: state.createTerminal,
    selectTerminal: state.selectTerminal,
    deleteTerminal: state.deleteTerminal,
    remoteConnections: state.remoteConnections,
    currentRemoteConnection: state.currentRemoteConnection,
    loadRemoteConnections: state.loadRemoteConnections,
    createRemoteConnection: state.createRemoteConnection,
    updateRemoteConnection: state.updateRemoteConnection,
    selectRemoteConnection: state.selectRemoteConnection,
    deleteRemoteConnection: state.deleteRemoteConnection,
    openRemoteSftp: state.openRemoteSftp,
  }), shallow);
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState('');
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [hasMoreLocked, setHasMoreLocked] = useState(false);
  const [sessionsExpanded, setSessionsExpanded] = useState(true);
  const [projectsExpanded, setProjectsExpanded] = useState(true);
  const [terminalsExpanded, setTerminalsExpanded] = useState(true);
  const [remoteExpanded, setRemoteExpanded] = useState(true);
  const [isRefreshingTerminals, setIsRefreshingTerminals] = useState(false);
  const [isRefreshingRemote, setIsRefreshingRemote] = useState(false);
  const PAGE_SIZE = 30;

  const [projectModalOpen, setProjectModalOpen] = useState(false);
  const [projectRoot, setProjectRoot] = useState('');
  const [projectError, setProjectError] = useState<string | null>(null);

  const [terminalModalOpen, setTerminalModalOpen] = useState(false);
  const [terminalRoot, setTerminalRoot] = useState('');
  const [terminalError, setTerminalError] = useState<string | null>(null);

  const [keyFilePickerOpen, setKeyFilePickerOpen] = useState(false);
  const [keyFilePickerTarget, setKeyFilePickerTarget] = useState<KeyFilePickerTarget>('private_key');
  const [keyFilePickerPath, setKeyFilePickerPath] = useState<string | null>(null);
  const [keyFilePickerParent, setKeyFilePickerParent] = useState<string | null>(null);
  const [keyFilePickerEntries, setKeyFilePickerEntries] = useState<FsEntry[]>([]);
  const [keyFilePickerRoots, setKeyFilePickerRoots] = useState<FsEntry[]>([]);
  const [keyFilePickerLoading, setKeyFilePickerLoading] = useState(false);
  const [keyFilePickerError, setKeyFilePickerError] = useState<string | null>(null);

  const [dirPickerOpen, setDirPickerOpen] = useState(false);
  const [dirPickerTarget, setDirPickerTarget] = useState<DirPickerTarget>('project');
  const [dirPickerPath, setDirPickerPath] = useState<string | null>(null);
  const [dirPickerParent, setDirPickerParent] = useState<string | null>(null);
  const [dirPickerEntries, setDirPickerEntries] = useState<FsEntry[]>([]);
  const [dirPickerRoots, setDirPickerRoots] = useState<FsEntry[]>([]);
  const [dirPickerLoading, setDirPickerLoading] = useState(false);
  const [dirPickerError, setDirPickerError] = useState<string | null>(null);
  const [showHiddenDirs, setShowHiddenDirs] = useState(false);
  const [dirPickerNewFolderName, setDirPickerNewFolderName] = useState('');
  const [dirPickerCreatingFolder, setDirPickerCreatingFolder] = useState(false);
  const [dirPickerCreateModalOpen, setDirPickerCreateModalOpen] = useState(false);

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

  const { sessionHasSummaryMap } = useSessionSummaryStatus({
    sessions,
    apiClient,
  });

  const didLoadProjectsRef = useRef(false);
  const didLoadTerminalsRef = useRef(false);
  const didLoadRemoteRef = useRef(false);
  
  const { dialogState, showConfirmDialog, handleConfirm, handleCancel } = useConfirmDialog();

  const isCollapsed = collapsed ?? !isOpen;
  const handleCreateSession = async () => {
    try {
      await createSession();
    } catch (error) {
      console.error('Failed to create session:', error);
    }
  };

  const handleSelectSession = async (sessionId: string) => {
    try {
      await selectSession(sessionId);
    } catch (error) {
      console.error('Failed to select session:', error);
    }
  };

  const closeActionMenus = useCallback((exceptMenu?: HTMLElement | null) => {
    if (typeof document === 'undefined') {
      return;
    }
    const menus = document.querySelectorAll<HTMLElement>('.js-inline-action-menu');
    menus.forEach((menu) => {
      if (exceptMenu && menu === exceptMenu) {
        return;
      }
      menu.classList.add('hidden');
    });
  }, []);

  const toggleActionMenu = useCallback((event: React.MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    const menu = event.currentTarget.nextElementSibling as HTMLElement | null;
    if (!menu) {
      return;
    }
    const shouldOpen = menu.classList.contains('hidden');
    closeActionMenus(menu);
    if (shouldOpen) {
      menu.classList.remove('hidden');
    } else {
      menu.classList.add('hidden');
    }
  }, [closeActionMenus]);

  const handleRefreshSessions = async () => {
    setIsRefreshing(true);
    const fetched = await loadSessions({ limit: PAGE_SIZE, offset: 0, append: false, silent: true });
    setIsRefreshing(false);
    setHasMoreLocked(false);
    setHasMore(fetched.length >= PAGE_SIZE);
  };

  const handleRefreshTerminals = async () => {
    setIsRefreshingTerminals(true);
    await loadTerminals();
    setIsRefreshingTerminals(false);
  };

  const handleRefreshRemote = async () => {
    setIsRefreshingRemote(true);
    await loadRemoteConnections();
    setIsRefreshingRemote(false);
  };

  const handleLoadMoreSessions = async () => {
    if (isLoadingMore) return;
    setIsLoadingMore(true);
    const fetched = await loadSessions({ limit: PAGE_SIZE, offset: sessions.length, append: true, silent: true });
    setIsLoadingMore(false);
    if (!fetched || fetched.length < PAGE_SIZE) {
      setHasMore(false);
      setHasMoreLocked(true);
    }
  };

  const openProjectModal = () => {
    setProjectRoot('');
    setProjectError(null);
    setProjectModalOpen(true);
  };

  const openTerminalModal = () => {
    setTerminalRoot('');
    setTerminalError(null);
    setTerminalModalOpen(true);
  };

  const openRemoteModal = () => {
    setKeyFilePickerOpen(false);
    openRemoteModalBase();
  };

  const handleCreateProject = async () => {
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
  };

  const handleCreateTerminal = async () => {
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
  };

  const handleSelectProject = async (projectId: string) => {
    try {
      await selectProject(projectId);
      await loadSessions({ limit: PAGE_SIZE, offset: 0, append: false, silent: true });
    } catch (error) {
      console.error('Failed to select project:', error);
    }
  };

  const handleSelectTerminal = async (terminalId: string) => {
    try {
      await selectTerminal(terminalId);
    } catch (error) {
      console.error('Failed to select terminal:', error);
    }
  };

  const handleSelectRemoteConnection = async (connectionId: string) => {
    try {
      await selectRemoteConnection(connectionId);
    } catch (error) {
      console.error('Failed to select remote connection:', error);
    }
  };

  const handleOpenRemoteSftp = async (connectionId: string) => {
    try {
      await openRemoteSftp(connectionId);
    } catch (error) {
      console.error('Failed to open remote sftp:', error);
    }
  };

  const focusTerminalPanel = () => {
    const targetTerminalId = currentTerminal?.id || terminals[0]?.id || null;
    if (targetTerminalId) {
      void handleSelectTerminal(targetTerminalId);
      return;
    }
    setActivePanel('terminal');
  };

  const focusRemotePanel = () => {
    const targetConnectionId = currentRemoteConnection?.id || remoteConnections[0]?.id || null;
    if (targetConnectionId) {
      void handleSelectRemoteConnection(targetConnectionId);
      return;
    }
    setActivePanel('remote_terminal');
  };

  const handleToggleSessionsSection = () => {
    setSessionsExpanded((prev) => {
      const next = !prev;
      if (next) {
        setProjectsExpanded(false);
        setTerminalsExpanded(false);
        setRemoteExpanded(false);
      }
      return next;
    });
  };

  const handleToggleProjectsSection = () => {
    setProjectsExpanded((prev) => {
      const next = !prev;
      if (next) {
        setSessionsExpanded(false);
        setTerminalsExpanded(false);
        setRemoteExpanded(false);
      }
      return next;
    });
  };

  const handleToggleTerminalsSection = () => {
    setTerminalsExpanded((prev) => {
      const next = !prev;
      if (next) {
        setSessionsExpanded(false);
        setProjectsExpanded(false);
        setRemoteExpanded(false);
        focusTerminalPanel();
      }
      return next;
    });
  };

  const handleToggleRemoteSection = () => {
    setRemoteExpanded((prev) => {
      const next = !prev;
      if (next) {
        setSessionsExpanded(false);
        setProjectsExpanded(false);
        setTerminalsExpanded(false);
        focusRemotePanel();
      }
      return next;
    });
  };

  const handleDeleteProject = async (projectId: string) => {
    const project = projects.find((p: Project) => p.id === projectId);
    showConfirmDialog({
      title: '删除确认',
      message: `确定要删除项目 "${project?.name || 'Untitled'}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          await deleteProject(projectId);
        } catch (error) {
          console.error('Failed to delete project:', error);
        }
      }
    });
  };

  const handleDeleteTerminal = async (terminalId: string) => {
    const terminal = terminals.find((t: Terminal) => t.id === terminalId);
    showConfirmDialog({
      title: '删除确认',
      message: `确定要删除终端 "${terminal?.name || 'Untitled'}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          await deleteTerminal(terminalId);
        } catch (error) {
          console.error('Failed to delete terminal:', error);
        }
      }
    });
  };

  const handleDeleteRemoteConnection = async (connectionId: string) => {
    const connection = remoteConnections.find((item: RemoteConnection) => item.id === connectionId);
    showConfirmDialog({
      title: '删除确认',
      message: `确定要删除远端连接 "${connection?.name || 'Untitled'}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          await deleteRemoteConnection(connectionId);
        } catch (error) {
          const feedback = resolveRemoteConnectionErrorFeedback(error, '删除远端连接失败');
          showConfirmDialog({
            title: '删除失败',
            message: feedback.message,
            description: feedback.message,
            detailsTitle: '建议操作',
            detailsLines: feedback.action ? [feedback.action] : undefined,
            confirmText: '知道了',
            cancelText: '关闭',
            type: 'info',
          });
        }
      }
    });
  };

  const loadDirEntries = async (path?: string | null) => {
    setDirPickerLoading(true);
    setDirPickerError(null);
    try {
      const data = await apiClient.listFsDirectories(path || undefined);
      setDirPickerPath(data?.path ?? null);
      setDirPickerParent(data?.parent ?? null);
      setDirPickerEntries(
        Array.isArray(data?.entries)
          ? data.entries.map((entry: any) => normalizeFsEntry(entry, true))
          : []
      );
      setDirPickerRoots(
        Array.isArray(data?.roots)
          ? data.roots.map((entry: any) => normalizeFsEntry(entry, true))
          : []
      );
    } catch (err: any) {
      setDirPickerError(err?.message || '加载目录失败');
    } finally {
      setDirPickerLoading(false);
    }
  };

  const openDirPicker = async (target: DirPickerTarget) => {
    setDirPickerTarget(target);
    setShowHiddenDirs(false);
    setDirPickerNewFolderName('');
    setDirPickerCreateModalOpen(false);
    setDirPickerError(null);
    setDirPickerOpen(true);
    const current = (target === 'project' ? projectRoot : terminalRoot).trim();
    await loadDirEntries(current ? current : null);
  };

  const closeDirPicker = () => {
    setDirPickerOpen(false);
    setDirPickerCreateModalOpen(false);
    setDirPickerNewFolderName('');
  };

  const openCreateDirModal = () => {
    if (!dirPickerPath) {
      setDirPickerError('请先进入一个父目录后再新建目录');
      return;
    }
    setDirPickerError(null);
    setDirPickerNewFolderName('');
    setDirPickerCreateModalOpen(true);
  };

  const createDirInPicker = async () => {
    const basePath = dirPickerPath;
    if (!basePath) {
      setDirPickerError('请先进入一个父目录后再新建目录');
      return;
    }
    const name = dirPickerNewFolderName.trim();
    if (!name) {
      setDirPickerError('请输入新目录名称');
      return;
    }

    setDirPickerCreatingFolder(true);
    setDirPickerError(null);
    try {
      const data = await apiClient.createFsDirectory(basePath, name);

      const apiPath = typeof data?.path === 'string' ? data.path.trim() : '';
      const fallbackSep = basePath.includes('\\') && !basePath.includes('/') ? '\\' : '/';
      const normalizedBase = basePath.replace(/[\\/]+$/, '');
      const createdPath = apiPath || `${normalizedBase}${fallbackSep}${name}`;

      setDirPickerNewFolderName('');
      setDirPickerCreateModalOpen(false);

      if (dirPickerTarget === 'project') {
        setProjectRoot(createdPath);
      } else {
        setTerminalRoot(createdPath);
      }

      await loadDirEntries(createdPath);
    } catch (err: any) {
      setDirPickerError(err?.message || '新建目录失败');
    } finally {
      setDirPickerCreatingFolder(false);
    }
  };

  const chooseDir = (path: string | null) => {
    if (!path) return;
    if (dirPickerTarget === 'project') {
      setProjectRoot(path);
    } else {
      setTerminalRoot(path);
    }
    closeDirPicker();
  };

  const loadKeyFileEntries = async (path?: string | null) => {
    setKeyFilePickerLoading(true);
    setKeyFilePickerError(null);
    try {
      const data = await apiClient.listFsEntries(path || undefined);
      setKeyFilePickerPath(data?.path ?? null);
      setKeyFilePickerParent(data?.parent ?? null);
      setKeyFilePickerEntries(
        Array.isArray(data?.entries)
          ? data.entries.map((entry: any) => normalizeFsEntry(entry, false))
          : []
      );
      setKeyFilePickerRoots(
        Array.isArray(data?.roots)
          ? data.roots.map((entry: any) => normalizeFsEntry(entry, false))
          : []
      );
    } catch (err: any) {
      setKeyFilePickerError(err?.message || '加载文件列表失败');
    } finally {
      setKeyFilePickerLoading(false);
    }
  };

  const openKeyFilePicker = async (target: KeyFilePickerTarget) => {
    setKeyFilePickerTarget(target);
    setKeyFilePickerError(null);
    setKeyFilePickerOpen(true);
    const currentPath = target === 'private_key'
      ? remotePrivateKeyPath
      : target === 'certificate'
        ? remoteCertificatePath
        : remoteJumpPrivateKeyPath;
    const parentPath = currentPath ? deriveParentPath(currentPath) : null;
    await loadKeyFileEntries(parentPath);
  };

  const closeKeyFilePicker = () => {
    setKeyFilePickerOpen(false);
    setKeyFilePickerError(null);
  };

  const applySelectedKeyFile = (path: string) => {
    if (!path) return;
    if (keyFilePickerTarget === 'private_key') {
      setRemotePrivateKeyPath(path);
    } else if (keyFilePickerTarget === 'certificate') {
      setRemoteCertificatePath(path);
    } else {
      setRemoteJumpPrivateKeyPath(path);
    }
    closeKeyFilePicker();
  };

  const handleDeleteSession = async (sessionId: string) => {
    const session = sessions.find((s: Session) => s.id === sessionId);
    if (!session || getSessionStatus(session) !== 'active') {
      return;
    }
    showConfirmDialog({
      title: '归档确认',
      message: `确定要归档会话 "${session.title || 'Untitled'}" 吗？归档后将不再参与总结。`,
      confirmText: '归档',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          await deleteSession(sessionId);
        } catch (error) {
          console.error('Failed to delete session:', error);
        }
      }
    });
  };

  const handleStartEdit = (sessionId: string, currentTitle: string) => {
    setEditingSessionId(sessionId);
    setEditingTitle(currentTitle);
  };

  const handleSaveEdit = async () => {
    if (editingSessionId && editingTitle.trim()) {
      try {
        await updateSession(editingSessionId, { title: editingTitle.trim() });
        setEditingSessionId(null);
        setEditingTitle('');
      } catch (error) {
        console.error('Failed to update session:', error);
      }
    }
  };

  const handleCancelEdit = () => {
    setEditingSessionId(null);
    setEditingTitle('');
  };

  const handleKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleSaveEdit();
    } else if (e.key === 'Escape') {
      handleCancelEdit();
    }
  };

  useEffect(() => {
    if (typeof document === 'undefined') {
      return;
    }

    const handlePointerDown = (event: MouseEvent | TouchEvent) => {
      const target = event.target as HTMLElement | null;
      if (!target) {
        return;
      }
      if (target.closest('[data-action-menu-root="true"]')) {
        return;
      }
      closeActionMenus();
    };

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        closeActionMenus();
      }
    };

    document.addEventListener('mousedown', handlePointerDown);
    document.addEventListener('touchstart', handlePointerDown);
    document.addEventListener('keydown', handleEscape);
    return () => {
      document.removeEventListener('mousedown', handlePointerDown);
      document.removeEventListener('touchstart', handlePointerDown);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [closeActionMenus]);

  useEffect(() => {
    if (hasMoreLocked) return;
    if (sessions.length === 0) return;
    setHasMore(sessions.length >= PAGE_SIZE);
  }, [sessions.length, hasMoreLocked]);

  useEffect(() => {
    if (didLoadProjectsRef.current) return;
    didLoadProjectsRef.current = true;
    loadProjects();
  }, [loadProjects]);

  useEffect(() => {
    if (didLoadTerminalsRef.current) return;
    didLoadTerminalsRef.current = true;
    loadTerminals();
  }, [loadTerminals]);

  useEffect(() => {
    if (didLoadRemoteRef.current) return;
    didLoadRemoteRef.current = true;
    loadRemoteConnections();
  }, [loadRemoteConnections]);

  useEffect(() => {
    if (isCollapsed || !terminalsExpanded) return;
    const timer = window.setInterval(() => {
      loadTerminals();
    }, 10000);
    return () => window.clearInterval(timer);
  }, [isCollapsed, terminalsExpanded, loadTerminals]);

  useEffect(() => {
    if (isCollapsed || !remoteExpanded) return;
    const timer = window.setInterval(() => {
      loadRemoteConnections();
    }, 12000);
    return () => window.clearInterval(timer);
  }, [isCollapsed, remoteExpanded, loadRemoteConnections]);

  const dirPickerItems = (dirPickerPath ? dirPickerEntries : dirPickerRoots)
    .filter((entry) => showHiddenDirs || !entry.name.startsWith('.'));
  const keyFilePickerItems = keyFilePickerPath ? keyFilePickerEntries : keyFilePickerRoots;
  const keyFilePickerTitle = getKeyFilePickerTitle(keyFilePickerTarget);

  return (
    <div
      className={cn(
        'flex flex-col h-full bg-card transition-all duration-200 overflow-hidden',
        isCollapsed ? 'w-0' : 'w-64 sm:w-72 border-r border-border',
        className
      )}
    >
      {/* 会话与项目列表 */}
      {!isCollapsed && (
        <div className="flex-1 flex flex-col overflow-hidden">
          <SessionSection
            expanded={sessionsExpanded}
            sessions={sessions}
            currentSessionId={currentSession?.id}
            sessionChatState={sessionChatState}
            taskReviewPanelsBySession={taskReviewPanelsBySession}
            uiPromptPanelsBySession={uiPromptPanelsBySession}
            sessionHasSummaryMap={sessionHasSummaryMap}
            editingSessionId={editingSessionId}
            editingTitle={editingTitle}
            hasMore={hasMore}
            isRefreshing={isRefreshing}
            isLoadingMore={isLoadingMore}
            summaryOpenSessionId={summaryOpenSessionId}
            onToggle={handleToggleSessionsSection}
            onRefresh={handleRefreshSessions}
            onCreateSession={handleCreateSession}
            onSelectSession={(sessionId) => {
              onSelectSession?.(sessionId);
              void handleSelectSession(sessionId);
            }}
            onOpenSummary={onOpenSummary}
            onStartEdit={handleStartEdit}
            onDeleteSession={handleDeleteSession}
            onEditingTitleChange={setEditingTitle}
            onSaveEdit={handleSaveEdit}
            onKeyPress={handleKeyPress}
            onLoadMore={handleLoadMoreSessions}
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
            onToggle={handleToggleProjectsSection}
            onCreate={openProjectModal}
            onSelect={(projectId) => {
              void handleSelectProject(projectId);
            }}
            onDelete={handleDeleteProject}
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

      <CreateProjectModal
        isOpen={projectModalOpen}
        projectRoot={projectRoot}
        projectError={projectError}
        onClose={() => setProjectModalOpen(false)}
        onProjectRootChange={setProjectRoot}
        onOpenPicker={() => {
          void openDirPicker('project');
        }}
        onCreate={() => {
          void handleCreateProject();
        }}
      />

      <CreateTerminalModal
        isOpen={terminalModalOpen}
        terminalRoot={terminalRoot}
        terminalError={terminalError}
        onClose={() => setTerminalModalOpen(false)}
        onTerminalRootChange={setTerminalRoot}
        onOpenPicker={() => {
          void openDirPicker('terminal');
        }}
        onCreate={() => {
          void handleCreateTerminal();
        }}
      />

      {/* 远端连接创建弹窗 */}
      <RemoteConnectionModal
        isOpen={remoteModalOpen}
        editingRemoteConnection={Boolean(editingRemoteConnectionId)}
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
        onClose={() => setRemoteModalOpen(false)}
        onRemoteNameChange={setRemoteName}
        onRemoteHostChange={setRemoteHost}
        onRemotePortChange={setRemotePort}
        onRemoteUsernameChange={setRemoteUsername}
        onRemoteAuthTypeChange={setRemoteAuthType}
        onRemotePasswordChange={setRemotePassword}
        onRemotePrivateKeyPathChange={setRemotePrivateKeyPath}
        onRemoteCertificatePathChange={setRemoteCertificatePath}
        onRemoteDefaultPathChange={setRemoteDefaultPath}
        onRemoteHostKeyPolicyChange={setRemoteHostKeyPolicy}
        onRemoteJumpEnabledChange={setRemoteJumpEnabled}
        onRemoteJumpHostChange={setRemoteJumpHost}
        onRemoteJumpPortChange={setRemoteJumpPort}
        onRemoteJumpUsernameChange={setRemoteJumpUsername}
        onRemoteJumpPrivateKeyPathChange={setRemoteJumpPrivateKeyPath}
        onRemoteJumpPasswordChange={setRemoteJumpPassword}
        onOpenKeyFilePicker={openKeyFilePicker}
        onTest={handleTestRemoteConnection}
        onSave={handleSaveRemoteConnection}
      />

      <KeyFilePickerDialog
        isOpen={keyFilePickerOpen}
        title={keyFilePickerTitle}
        currentPath={keyFilePickerPath}
        parentPath={keyFilePickerParent}
        loading={keyFilePickerLoading}
        items={keyFilePickerItems}
        error={keyFilePickerError}
        onClose={closeKeyFilePicker}
        onBack={() => loadKeyFileEntries(keyFilePickerParent)}
        onRefresh={() => loadKeyFileEntries(keyFilePickerPath)}
        onEntryClick={(entry) => {
          if (entry.isDir) {
            void loadKeyFileEntries(entry.path);
          } else {
            applySelectedKeyFile(entry.path);
          }
        }}
        onSelectFile={applySelectedKeyFile}
      />

      <DirPickerDialog
        isOpen={dirPickerOpen}
        target={dirPickerTarget}
        currentPath={dirPickerPath}
        parentPath={dirPickerParent}
        loading={dirPickerLoading}
        items={dirPickerItems}
        error={dirPickerError}
        showHiddenDirs={showHiddenDirs}
        createModalOpen={dirPickerCreateModalOpen}
        newFolderName={dirPickerNewFolderName}
        creatingFolder={dirPickerCreatingFolder}
        onClose={closeDirPicker}
        onBack={() => loadDirEntries(dirPickerParent)}
        onChooseCurrent={() => chooseDir(dirPickerPath)}
        onOpenCreateModal={openCreateDirModal}
        onToggleHiddenDirs={() => setShowHiddenDirs((prev) => !prev)}
        onOpenEntry={(path) => loadDirEntries(path)}
        onCreateModalClose={() => setDirPickerCreateModalOpen(false)}
        onNewFolderNameChange={setDirPickerNewFolderName}
        onCreateDir={createDirInPicker}
      />

      {/* 确认对话框 */}
      <ConfirmDialog
        isOpen={dialogState.isOpen}
        title={dialogState.title}
        message={dialogState.message}
        description={dialogState.description}
        details={dialogState.details}
        detailsTitle={dialogState.detailsTitle}
        detailsLines={dialogState.detailsLines}
        confirmText={dialogState.confirmText}
        cancelText={dialogState.cancelText}
        type={dialogState.type}
        onConfirm={handleConfirm}
        onCancel={handleCancel}
      />
    </div>
  );
};
