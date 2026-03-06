import React, { useCallback, useState, useEffect, useRef } from 'react';
import { useChatStoreFromContext, useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import { apiClient as globalApiClient } from '../lib/api/client';
import type { Session, Project, FsEntry, Terminal, RemoteConnection } from '../types';
import ConfirmDialog from './ui/ConfirmDialog';
import { useConfirmDialog } from '../hooks/useConfirmDialog';
import { cn } from '../lib/utils';
import { DirPickerDialog, KeyFilePickerDialog } from './sessionList/Pickers';
import { RemoteConnectionModal } from './sessionList/RemoteConnectionModal';
import {
  ProjectSection,
  RemoteSection,
  SessionSection,
  TerminalSection,
} from './sessionList/Sections';
import {
  buildRemoteConnectionPayload,
  deriveNameFromPath,
  deriveParentPath,
  getKeyFilePickerTitle,
  normalizeFsEntry,
} from './sessionList/helpers';
import type {
  DirPickerTarget,
  HostKeyPolicy,
  KeyFilePickerTarget,
  RemoteAuthType,
} from './sessionList/helpers';

// 简化的时间格式化函数
const formatTimeAgo = (date: string | Date | undefined | null) => {
  const now = new Date();
  let past: Date;
  
  // 处理不同的日期格式
  if (!date) {
    return '时间未知';
  }
  
  if (typeof date === 'string') {
    // 处理数据库返回的时间格式 "YYYY-MM-DD HH:mm:ss"
    // 将其转换为ISO格式以便正确解析
    const isoString = date.replace(' ', 'T') + 'Z';
    past = new Date(isoString);
    
    // 如果ISO格式解析失败，尝试直接解析原字符串
    if (isNaN(past.getTime())) {
      past = new Date(date);
    }
  } else {
    past = date;
  }
  
  // 检查日期是否有效
  if (!past || isNaN(past.getTime())) {
    return '时间未知';
  }
  
  const diffInSeconds = Math.floor((now.getTime() - past.getTime()) / 1000);
  
  if (diffInSeconds < 60) return '刚刚';
  if (diffInSeconds < 3600) return `${Math.floor(diffInSeconds / 60)}分钟前`;
  if (diffInSeconds < 86400) return `${Math.floor(diffInSeconds / 3600)}小时前`;
  if (diffInSeconds < 2592000) return `${Math.floor(diffInSeconds / 86400)}天前`;
  return past.toLocaleDateString('zh-CN');
};

const getSessionStatus = (session: Session): 'active' | 'archiving' | 'archived' => {
  const rawStatus = typeof session.status === 'string' ? session.status.toLowerCase() : '';
  if (rawStatus === 'archiving') return 'archiving';
  if (rawStatus === 'archived') return 'archived';
  if (session.archived) return 'archived';
  return 'active';
};

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
  // 尝试从Context获取store（如果可用）
  let contextStore = null;
  try {
    contextStore = useChatStoreFromContext();
  } catch (error) {
    // 如果Context不可用，contextStore保持为null
  }
  
  const storeToUse = store ? store() : contextStore;
  
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
  } = storeToUse;
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState('');
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [hasMoreLocked, setHasMoreLocked] = useState(false);
  const [sessionHasSummaryMap, setSessionHasSummaryMap] = useState<Record<string, boolean>>({});
  const checkingSummaryIdsRef = useRef<Set<string>>(new Set());
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

  const [remoteModalOpen, setRemoteModalOpen] = useState(false);
  const [remoteName, setRemoteName] = useState('');
  const [remoteHost, setRemoteHost] = useState('');
  const [remotePort, setRemotePort] = useState('22');
  const [remoteUsername, setRemoteUsername] = useState('');
  const [remoteAuthType, setRemoteAuthType] = useState<RemoteAuthType>('private_key');
  const [remotePassword, setRemotePassword] = useState('');
  const [remotePrivateKeyPath, setRemotePrivateKeyPath] = useState('');
  const [remoteCertificatePath, setRemoteCertificatePath] = useState('');
  const [remoteDefaultPath, setRemoteDefaultPath] = useState('');
  const [remoteHostKeyPolicy, setRemoteHostKeyPolicy] = useState<HostKeyPolicy>('strict');
  const [remoteJumpEnabled, setRemoteJumpEnabled] = useState(false);
  const [remoteJumpHost, setRemoteJumpHost] = useState('');
  const [remoteJumpPort, setRemoteJumpPort] = useState('22');
  const [remoteJumpUsername, setRemoteJumpUsername] = useState('');
  const [remoteJumpPrivateKeyPath, setRemoteJumpPrivateKeyPath] = useState('');
  const [remoteJumpPassword, setRemoteJumpPassword] = useState('');
  const [remoteError, setRemoteError] = useState<string | null>(null);
  const [remoteSuccess, setRemoteSuccess] = useState<string | null>(null);
  const [remoteTesting, setRemoteTesting] = useState(false);
  const [remoteSaving, setRemoteSaving] = useState(false);
  const [editingRemoteConnectionId, setEditingRemoteConnectionId] = useState<string | null>(null);

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
    setEditingRemoteConnectionId(null);
    setRemoteName('');
    setRemoteHost('');
    setRemotePort('22');
    setRemoteUsername('');
    setRemoteAuthType('private_key');
    setRemotePassword('');
    setRemotePrivateKeyPath('');
    setRemoteCertificatePath('');
    setRemoteDefaultPath('');
    setRemoteHostKeyPolicy('strict');
    setRemoteJumpEnabled(false);
    setRemoteJumpHost('');
    setRemoteJumpPort('22');
    setRemoteJumpUsername('');
    setRemoteJumpPrivateKeyPath('');
    setRemoteJumpPassword('');
    setRemoteError(null);
    setRemoteSuccess(null);
    setRemoteTesting(false);
    setRemoteSaving(false);
    setKeyFilePickerOpen(false);
    setRemoteModalOpen(true);
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

  const handleTestRemoteConnection = async () => {
    const built = buildRemoteConnectionPayload({
      name: remoteName,
      host: remoteHost,
      port: remotePort,
      username: remoteUsername,
      authType: remoteAuthType,
      password: remotePassword,
      privateKeyPath: remotePrivateKeyPath,
      certificatePath: remoteCertificatePath,
      defaultPath: remoteDefaultPath,
      hostKeyPolicy: remoteHostKeyPolicy,
      jumpEnabled: remoteJumpEnabled,
      jumpHost: remoteJumpHost,
      jumpPort: remoteJumpPort,
      jumpUsername: remoteJumpUsername,
      jumpPrivateKeyPath: remoteJumpPrivateKeyPath,
      jumpPassword: remoteJumpPassword,
    });
    if ('error' in built) {
      setRemoteError(built.error);
      setRemoteSuccess(null);
      return;
    }
    setRemoteTesting(true);
    setRemoteError(null);
    setRemoteSuccess(null);
    try {
      const result = await apiClient.testRemoteConnectionDraft(built.payload);
      const remoteHostName = result?.remote_host ? ` (${result.remote_host})` : '';
      setRemoteSuccess(`连接测试成功${remoteHostName}`);
    } catch (error) {
      setRemoteError(error instanceof Error ? error.message : '连接测试失败');
    } finally {
      setRemoteTesting(false);
    }
  };

  const handleSaveRemoteConnection = async () => {
    const built = buildRemoteConnectionPayload({
      name: remoteName,
      host: remoteHost,
      port: remotePort,
      username: remoteUsername,
      authType: remoteAuthType,
      password: remotePassword,
      privateKeyPath: remotePrivateKeyPath,
      certificatePath: remoteCertificatePath,
      defaultPath: remoteDefaultPath,
      hostKeyPolicy: remoteHostKeyPolicy,
      jumpEnabled: remoteJumpEnabled,
      jumpHost: remoteJumpHost,
      jumpPort: remoteJumpPort,
      jumpUsername: remoteJumpUsername,
      jumpPrivateKeyPath: remoteJumpPrivateKeyPath,
      jumpPassword: remoteJumpPassword,
    });
    if ('error' in built) {
      setRemoteError(built.error);
      setRemoteSuccess(null);
      return;
    }
    setRemoteSaving(true);
    setRemoteError(null);
    setRemoteSuccess(null);
    try {
      if (editingRemoteConnectionId) {
        const updated = await updateRemoteConnection(editingRemoteConnectionId, built.payload);
        if (!updated) {
          throw new Error('更新远端连接失败');
        }
      } else {
        await createRemoteConnection(built.payload);
      }
      setRemoteModalOpen(false);
    } catch (error) {
      setRemoteError(error instanceof Error ? error.message : (editingRemoteConnectionId ? '更新远端连接失败' : '创建远端连接失败'));
    } finally {
      setRemoteSaving(false);
    }
  };

  const openEditRemoteModal = (connection: RemoteConnection) => {
    setEditingRemoteConnectionId(connection.id);
    setRemoteName(connection.name || '');
    setRemoteHost(connection.host || '');
    setRemotePort(String(connection.port || 22));
    setRemoteUsername(connection.username || '');
    setRemoteAuthType(connection.authType || 'private_key');
    setRemotePassword(connection.password || '');
    setRemotePrivateKeyPath(connection.privateKeyPath || '');
    setRemoteCertificatePath(connection.certificatePath || '');
    setRemoteDefaultPath(connection.defaultRemotePath || '');
    setRemoteHostKeyPolicy(connection.hostKeyPolicy || 'strict');
    setRemoteJumpEnabled(Boolean(connection.jumpEnabled));
    setRemoteJumpHost(connection.jumpHost || '');
    setRemoteJumpPort(String(connection.jumpPort || 22));
    setRemoteJumpUsername(connection.jumpUsername || '');
    setRemoteJumpPrivateKeyPath(connection.jumpPrivateKeyPath || '');
    setRemoteJumpPassword(connection.jumpPassword || '');
    setRemoteError(null);
    setRemoteSuccess(null);
    setRemoteTesting(false);
    setRemoteSaving(false);
    setRemoteModalOpen(true);
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

  const handleQuickTestRemoteConnection = async (connection: RemoteConnection) => {
    try {
      await apiClient.testRemoteConnection(connection.id);
      setRemoteSuccess(`连接测试成功 (${connection.name})`);
      setRemoteError(null);
    } catch (error) {
      setRemoteError(error instanceof Error ? error.message : '连接测试失败');
    }
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
          console.error('Failed to delete remote connection:', error);
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
    const validIds = new Set(sessions.map((session: Session) => session.id));
    checkingSummaryIdsRef.current.forEach((sessionId) => {
      if (!validIds.has(sessionId)) {
        checkingSummaryIdsRef.current.delete(sessionId);
      }
    });

    setSessionHasSummaryMap((prev) => {
      const next: Record<string, boolean> = {};
      let changed = false;
      Object.entries(prev).forEach(([sessionId, hasSummary]) => {
        if (validIds.has(sessionId)) {
          next[sessionId] = hasSummary;
        } else {
          changed = true;
        }
      });
      return changed ? next : prev;
    });
  }, [sessions]);

  const checkSessionSummaryStatus = useCallback(async (sessionIds: string[]) => {
    const uniqueSessionIds = Array.from(new Set(
      sessionIds
        .map((sessionId) => String(sessionId || '').trim())
        .filter((sessionId) => sessionId.length > 0)
    ));
    const pendingSessionIds = uniqueSessionIds.filter(
      (sessionId) => !checkingSummaryIdsRef.current.has(sessionId)
    );
    if (pendingSessionIds.length === 0) {
      return;
    }

    pendingSessionIds.forEach((sessionId) => checkingSummaryIdsRef.current.add(sessionId));
    try {
      const pairs = await Promise.all(
        pendingSessionIds.map(async (sessionId) => {
          try {
            const payload = await apiClient.getSessionSummaries(sessionId, { limit: 1, offset: 0 });
            const hasSummary = payload?.has_summary === true
              || (Array.isArray(payload?.items) && payload.items.length > 0);
            return { sessionId, hasSummary };
          } catch (error) {
            console.warn('Failed to detect session summary status:', sessionId, error);
            return { sessionId, hasSummary: false };
          }
        })
      );

      setSessionHasSummaryMap((prev) => {
        const next = { ...prev };
        let changed = false;
        pairs.forEach(({ sessionId, hasSummary }) => {
          if (next[sessionId] !== hasSummary) {
            next[sessionId] = hasSummary;
            changed = true;
          }
        });
        return changed ? next : prev;
      });
    } finally {
      pendingSessionIds.forEach((sessionId) => checkingSummaryIdsRef.current.delete(sessionId));
    }
  }, [apiClient]);

  useEffect(() => {
    if (sessions.length === 0) {
      return;
    }

    const unknownSessionIds = sessions
      .filter((session: Session) => getSessionStatus(session) === 'active')
      .map((session: Session) => session.id)
      .filter((sessionId) => (
        typeof sessionHasSummaryMap[sessionId] !== 'boolean'
      ));
    if (unknownSessionIds.length === 0) {
      return;
    }

    void checkSessionSummaryStatus(unknownSessionIds);
  }, [checkSessionSummaryStatus, sessionHasSummaryMap, sessions]);

  useEffect(() => {
    if (sessions.length === 0) {
      return;
    }

    const sessionIds = sessions
      .filter((session: Session) => getSessionStatus(session) === 'active')
      .map((session: Session) => session.id);
    if (sessionIds.length === 0) {
      return;
    }
    void checkSessionSummaryStatus(sessionIds);

    const timer = window.setInterval(() => {
      void checkSessionSummaryStatus(sessionIds);
    }, 30000);
    return () => window.clearInterval(timer);
  }, [checkSessionSummaryStatus, sessions]);

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
            onEdit={openEditRemoteModal}
            onTest={handleQuickTestRemoteConnection}
            onDelete={handleDeleteRemoteConnection}
            onToggleActionMenu={toggleActionMenu}
            closeActionMenus={() => closeActionMenus()}
            formatTimeAgo={formatTimeAgo}
          />
        </div>
      )}

      {/* 项目创建弹窗 */}
      {projectModalOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="fixed inset-0 bg-black/50" onClick={() => setProjectModalOpen(false)} />
          <div className="relative bg-card border border-border rounded-lg shadow-xl w-[520px] p-6">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold text-foreground">新增项目</h3>
              <button
                onClick={() => setProjectModalOpen(false)}
                className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
              >
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div className="space-y-4">
              <div>
                <label className="text-sm text-muted-foreground">项目目录</label>
                <div className="mt-1 flex items-center gap-2">
                  <input
                    value={projectRoot}
                    onChange={(e) => setProjectRoot(e.target.value)}
                    className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="选择或输入本地目录路径"
                  />
                  <button
                    type="button"
                    onClick={() => openDirPicker('project')}
                    className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
                  >
                    选择目录
                  </button>
                </div>
              </div>
              {projectRoot.trim() && (
                <div className="text-xs text-muted-foreground">
                  项目名称将默认使用：<span className="text-foreground">{deriveNameFromPath(projectRoot, 'Project')}</span>
                </div>
              )}
              {projectError && (
                <div className="text-xs text-destructive">{projectError}</div>
              )}
            </div>
            <div className="mt-6 flex justify-end gap-2">
              <button
                onClick={() => setProjectModalOpen(false)}
                className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
              >
                取消
              </button>
              <button
                onClick={handleCreateProject}
                className="px-4 py-2 rounded bg-primary text-primary-foreground hover:bg-primary/90"
              >
                创建
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 终端创建弹窗 */}
      {terminalModalOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="fixed inset-0 bg-black/50" onClick={() => setTerminalModalOpen(false)} />
          <div className="relative bg-card border border-border rounded-lg shadow-xl w-[520px] p-6">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold text-foreground">新增终端</h3>
              <button
                onClick={() => setTerminalModalOpen(false)}
                className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
              >
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div className="space-y-4">
              <div>
                <label className="text-sm text-muted-foreground">终端目录</label>
                <div className="mt-1 flex items-center gap-2">
                  <input
                    value={terminalRoot}
                    onChange={(e) => setTerminalRoot(e.target.value)}
                    className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="选择或输入本地目录路径"
                  />
                  <button
                    type="button"
                    onClick={() => openDirPicker('terminal')}
                    className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
                  >
                    选择目录
                  </button>
                </div>
              </div>
              {terminalRoot.trim() && (
                <div className="text-xs text-muted-foreground">
                  终端名称将默认使用：<span className="text-foreground">{deriveNameFromPath(terminalRoot, 'Terminal')}</span>
                </div>
              )}
              {terminalError && (
                <div className="text-xs text-destructive">{terminalError}</div>
              )}
            </div>
            <div className="mt-6 flex justify-end gap-2">
              <button
                onClick={() => setTerminalModalOpen(false)}
                className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
              >
                取消
              </button>
              <button
                onClick={handleCreateTerminal}
                className="px-4 py-2 rounded bg-primary text-primary-foreground hover:bg-primary/90"
              >
                创建
              </button>
            </div>
          </div>
        </div>
      )}

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
        confirmText={dialogState.confirmText}
        cancelText={dialogState.cancelText}
        type={dialogState.type}
        onConfirm={handleConfirm}
        onCancel={handleCancel}
      />
    </div>
  );
};
