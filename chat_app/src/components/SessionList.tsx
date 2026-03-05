import React, { useCallback, useState, useEffect, useRef } from 'react';
import { useChatStoreFromContext, useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import { apiClient as globalApiClient } from '../lib/api/client';
import type { Session, Project, FsEntry, Terminal, RemoteConnection } from '../types';
import { PlusIcon, DotsVerticalIcon, PencilIcon, TrashIcon, ChatIcon } from './ui/icons';
import ConfirmDialog from './ui/ConfirmDialog';
import { useConfirmDialog } from '../hooks/useConfirmDialog';
import { cn } from '../lib/utils';

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

interface RemoteConnectionFormPayload {
  name?: string;
  host: string;
  port?: number;
  username: string;
  auth_type?: 'private_key' | 'private_key_cert' | 'password';
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: 'strict' | 'accept_new';
  jump_enabled?: boolean;
  jump_host?: string;
  jump_port?: number;
  jump_username?: string;
  jump_private_key_path?: string;
  jump_password?: string;
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
  const [remoteAuthType, setRemoteAuthType] = useState<'private_key' | 'private_key_cert' | 'password'>('private_key');
  const [remotePassword, setRemotePassword] = useState('');
  const [remotePrivateKeyPath, setRemotePrivateKeyPath] = useState('');
  const [remoteCertificatePath, setRemoteCertificatePath] = useState('');
  const [remoteDefaultPath, setRemoteDefaultPath] = useState('');
  const [remoteHostKeyPolicy, setRemoteHostKeyPolicy] = useState<'strict' | 'accept_new'>('strict');
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
  const [keyFilePickerTarget, setKeyFilePickerTarget] = useState<'private_key' | 'certificate' | 'jump_private_key'>('private_key');
  const [keyFilePickerPath, setKeyFilePickerPath] = useState<string | null>(null);
  const [keyFilePickerParent, setKeyFilePickerParent] = useState<string | null>(null);
  const [keyFilePickerEntries, setKeyFilePickerEntries] = useState<FsEntry[]>([]);
  const [keyFilePickerRoots, setKeyFilePickerRoots] = useState<FsEntry[]>([]);
  const [keyFilePickerLoading, setKeyFilePickerLoading] = useState(false);
  const [keyFilePickerError, setKeyFilePickerError] = useState<string | null>(null);

  const [dirPickerOpen, setDirPickerOpen] = useState(false);
  const [dirPickerTarget, setDirPickerTarget] = useState<'project' | 'terminal'>('project');
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

  const deriveProjectName = (path: string) => {
    const trimmed = path.trim().replace(/[\\/]+$/, '');
    if (!trimmed) return 'Project';
    const parts = trimmed.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] || 'Project';
  };

  const deriveTerminalName = (path: string) => {
    const trimmed = path.trim().replace(/[\\/]+$/, '');
    if (!trimmed) return 'Terminal';
    const parts = trimmed.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] || 'Terminal';
  };

  const handleCreateProject = async () => {
    if (!projectRoot.trim()) {
      setProjectError('请选择项目目录');
      return;
    }
    try {
      const name = deriveProjectName(projectRoot);
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
      const name = deriveTerminalName(terminalRoot);
      await createTerminal(terminalRoot.trim(), name);
      setTerminalModalOpen(false);
    } catch (error) {
      setTerminalError(error instanceof Error ? error.message : '创建终端失败');
    }
  };

  const buildRemoteConnectionPayload = (): { payload: RemoteConnectionFormPayload } | { error: string } => {
    if (!remoteHost.trim()) {
      return { error: '请输入主机地址' };
    }
    if (!remoteUsername.trim()) {
      return { error: '请输入用户名' };
    }
    if (remoteAuthType === 'password' && !remotePassword.trim()) {
      return { error: '密码模式需要填写密码' };
    }
    if (remoteAuthType !== 'password' && !remotePrivateKeyPath.trim()) {
      return { error: '请输入私钥路径' };
    }
    if (remoteAuthType === 'private_key_cert' && !remoteCertificatePath.trim()) {
      return { error: '私钥+证书模式需要证书路径' };
    }
    if (remoteJumpEnabled && (!remoteJumpHost.trim() || !remoteJumpUsername.trim())) {
      return { error: '启用跳板机后需填写跳板机主机和用户名' };
    }

    const parsedPort = Number(remotePort);
    if (!Number.isFinite(parsedPort) || parsedPort < 1 || parsedPort > 65535) {
      return { error: '端口范围必须在 1-65535' };
    }
    const parsedJumpPort = Number(remoteJumpPort);
    if (remoteJumpEnabled && (!Number.isFinite(parsedJumpPort) || parsedJumpPort < 1 || parsedJumpPort > 65535)) {
      return { error: '跳板机端口范围必须在 1-65535' };
    }

    const defaultName = `${remoteUsername.trim()}@${remoteHost.trim()}`;
    return {
      payload: {
        name: remoteName.trim() || defaultName,
        host: remoteHost.trim(),
        port: parsedPort,
        username: remoteUsername.trim(),
        auth_type: remoteAuthType,
        password: remoteAuthType === 'password' ? remotePassword : undefined,
        private_key_path: remoteAuthType === 'password' ? undefined : remotePrivateKeyPath.trim(),
        certificate_path: remoteAuthType === 'private_key_cert' ? remoteCertificatePath.trim() : undefined,
        default_remote_path: remoteDefaultPath.trim() || undefined,
        host_key_policy: remoteHostKeyPolicy,
        jump_enabled: remoteJumpEnabled,
        jump_host: remoteJumpEnabled ? remoteJumpHost.trim() : undefined,
        jump_port: remoteJumpEnabled ? parsedJumpPort : undefined,
        jump_username: remoteJumpEnabled ? remoteJumpUsername.trim() : undefined,
        jump_private_key_path: remoteJumpEnabled && remoteJumpPrivateKeyPath.trim()
          ? remoteJumpPrivateKeyPath.trim()
          : undefined,
        jump_password: remoteJumpEnabled && remoteJumpPassword.trim()
          ? remoteJumpPassword
          : undefined,
      },
    };
  };

  const handleTestRemoteConnection = async () => {
    const built = buildRemoteConnectionPayload();
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
    const built = buildRemoteConnectionPayload();
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
      const mapEntry = (entry: any): FsEntry => ({
        name: entry?.name ?? '',
        path: entry?.path ?? '',
        isDir: entry?.is_dir ?? entry?.isDir ?? true,
        size: entry?.size ?? null,
        modifiedAt: entry?.modified_at ?? entry?.modifiedAt ?? null,
      });
      setDirPickerPath(data?.path ?? null);
      setDirPickerParent(data?.parent ?? null);
      setDirPickerEntries(Array.isArray(data?.entries) ? data.entries.map(mapEntry) : []);
      setDirPickerRoots(Array.isArray(data?.roots) ? data.roots.map(mapEntry) : []);
    } catch (err: any) {
      setDirPickerError(err?.message || '加载目录失败');
    } finally {
      setDirPickerLoading(false);
    }
  };

  const openDirPicker = async (target: 'project' | 'terminal') => {
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

  const deriveParentPath = (path: string) => {
    const normalized = path.trim().replace(/[\\/]+$/, '');
    if (!normalized) return null;
    const idx = Math.max(normalized.lastIndexOf('/'), normalized.lastIndexOf('\\'));
    if (idx < 0) return null;
    if (idx === 0) return normalized[0];
    const parent = normalized.slice(0, idx);
    if (/^[A-Za-z]:$/.test(parent)) {
      return `${parent}\\`;
    }
    return parent;
  };

  const loadKeyFileEntries = async (path?: string | null) => {
    setKeyFilePickerLoading(true);
    setKeyFilePickerError(null);
    try {
      const data = await apiClient.listFsEntries(path || undefined);
      const mapEntry = (entry: any): FsEntry => ({
        name: entry?.name ?? '',
        path: entry?.path ?? '',
        isDir: entry?.is_dir ?? entry?.isDir ?? false,
        size: entry?.size ?? null,
        modifiedAt: entry?.modified_at ?? entry?.modifiedAt ?? null,
      });
      setKeyFilePickerPath(data?.path ?? null);
      setKeyFilePickerParent(data?.parent ?? null);
      setKeyFilePickerEntries(Array.isArray(data?.entries) ? data.entries.map(mapEntry) : []);
      setKeyFilePickerRoots(Array.isArray(data?.roots) ? data.roots.map(mapEntry) : []);
    } catch (err: any) {
      setKeyFilePickerError(err?.message || '加载文件列表失败');
    } finally {
      setKeyFilePickerLoading(false);
    }
  };

  const openKeyFilePicker = async (target: 'private_key' | 'certificate' | 'jump_private_key') => {
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
  const keyFilePickerTitle = keyFilePickerTarget === 'private_key'
    ? '选择私钥文件'
    : keyFilePickerTarget === 'certificate'
      ? '选择证书文件'
      : '选择跳板机私钥文件';

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
          <div className={cn('flex flex-col min-h-0', sessionsExpanded ? 'flex-1' : 'shrink-0')}>
            <div className="px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
            <button
              type="button"
              onClick={() => {
                setSessionsExpanded((prev) => {
                  const next = !prev;
                  if (next) {
                    setProjectsExpanded(false);
                    setTerminalsExpanded(false);
                    setRemoteExpanded(false);
                  }
                  return next;
                });
              }}
              className="flex items-center gap-2 uppercase tracking-wide"
            >
              <span>{sessionsExpanded ? '▾' : '▸'}</span>
              <span>SESSIONS</span>
            </button>
            <div className="flex items-center gap-1">
              <button
                onClick={handleRefreshSessions}
                className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
                title="刷新会话列表"
              >
                <svg className={cn('w-4 h-4', isRefreshing && 'animate-spin')} fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12a7.5 7.5 0 0112.125-5.303M19.5 12a7.5 7.5 0 01-12.125 5.303M16.5 6.697V3m0 3.697h-3.697M7.5 17.303V21m0-3.697H3.803" />
                </svg>
              </button>
              <button
                onClick={handleCreateSession}
                className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
                title="新建会话"
              >
                <PlusIcon className="w-4 h-4" />
              </button>
            </div>
            </div>
            {sessionsExpanded && (
              <div className="flex-1 min-h-0 overflow-y-auto">
                {sessions.length === 0 ? (
                  <div className="flex flex-col items-center justify-center text-muted-foreground py-6">
                    <ChatIcon className="w-12 h-12 mb-4 opacity-50" />
                    <p className="text-sm">还没有会话</p>
                    <button
                      onClick={handleCreateSession}
                      className="mt-2 px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
                    >
                      创建第一个会话
                    </button>
                  </div>
                ) : (
                  <div className="p-2 space-y-1">
                    {sessions.map((session: Session) => {
                      const sessionStatus = getSessionStatus(session);
                      const isArchivedSession = sessionStatus !== 'active';
                      const isArchivingSession = sessionStatus === 'archiving';

                      return (
                        <div
                          key={session.id}
                          className={cn(
                            'group relative flex items-center p-3 rounded-lg transition-colors',
                            isArchivedSession ? 'cursor-default opacity-70' : 'cursor-pointer',
                            currentSession?.id === session.id
                              ? 'bg-accent border border-border'
                              : (!isArchivedSession && 'hover:bg-accent/50'),
                          )}
                          onClick={() => {
                            if (isArchivedSession) {
                              return;
                            }
                            onSelectSession?.(session.id);
                            void handleSelectSession(session.id);
                          }}
                        >
                          <div className="flex-1 min-w-0">
                            {editingSessionId === session.id ? (
                              <input
                                type="text"
                                value={editingTitle}
                                onChange={(e) => setEditingTitle(e.target.value)}
                                onBlur={handleSaveEdit}
                                onKeyDown={handleKeyPress}
                                className="w-full px-2 py-1 text-sm bg-background border border-border rounded focus:outline-none focus:ring-2 focus:ring-ring"
                                autoFocus
                                onClick={(e) => e.stopPropagation()}
                              />
                            ) : (
                              <>
                                <h3 className="text-sm font-medium text-foreground truncate">
                                  {session.title}
                                </h3>
                                <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
                                  <span>{formatTimeAgo(session.updatedAt)}</span>
                                  <span className="text-muted-foreground/60">·</span>
                                  {isArchivedSession ? (
                                    <span className={cn(
                                      'inline-flex items-center gap-1',
                                      isArchivingSession ? 'text-amber-600' : 'text-slate-500'
                                    )}>
                                      <span className={cn(
                                        'inline-block w-2 h-2 rounded-full',
                                        isArchivingSession ? 'bg-amber-500 animate-pulse' : 'bg-slate-400'
                                      )} />
                                      {isArchivingSession ? '归档中' : '已归档'}
                                    </span>
                                  ) : (
                                    (() => {
                                      const chatState = sessionChatState?.[session.id];
                                      const isBusy = !!(chatState?.isLoading || chatState?.isStreaming);
                                      return (
                                        <span className={cn('inline-flex items-center gap-1', isBusy ? 'text-amber-600' : 'text-muted-foreground')}>
                                          <span className={cn('inline-block w-2 h-2 rounded-full', isBusy ? 'bg-amber-500' : 'bg-muted-foreground/40')} />
                                          {isBusy ? '执行中' : '空闲'}
                                        </span>
                                      );
                                    })()
                                  )}
                                  {(() => {
                                    if (isArchivedSession) {
                                      return null;
                                    }
                                    const taskReviewCount = Array.isArray(taskReviewPanelsBySession?.[session.id])
                                      ? taskReviewPanelsBySession[session.id].length
                                      : 0;
                                    const uiPromptCount = Array.isArray(uiPromptPanelsBySession?.[session.id])
                                      ? uiPromptPanelsBySession[session.id].length
                                      : 0;
                                    const pendingCount = taskReviewCount + uiPromptCount;
                                    if (pendingCount <= 0) {
                                      return null;
                                    }
                                    return (
                                      <span className="inline-flex items-center gap-1 text-blue-600">
                                        <span className="inline-block w-2 h-2 rounded-full bg-blue-500 animate-pulse" />
                                        {`\u5f85\u5904\u7406 ${pendingCount}`}
                                      </span>
                                    );
                                  })()}
                                  {!isArchivedSession && sessionHasSummaryMap[session.id] && (
                                    <button
                                      type="button"
                                      className={cn(
                                        'inline-flex items-center rounded border px-1.5 py-0.5 text-[10px] transition-colors',
                                        summaryOpenSessionId === session.id
                                          ? 'border-blue-500/50 bg-blue-500/10 text-blue-600'
                                          : 'border-blue-500/30 text-blue-600 hover:bg-blue-500/10'
                                      )}
                                      onClick={(event) => {
                                        event.stopPropagation();
                                        void handleSelectSession(session.id);
                                        onOpenSummary?.(session.id);
                                      }}
                                      title="查看会话总结"
                                    >
                                      总结
                                    </button>
                                  )}
                                </div>
                              </>
                            )}
                          </div>

                          {/* 操作菜单 */}
                          {editingSessionId !== session.id && (
                            <div className="relative" data-action-menu-root="true">
                              <button
                                className="p-1 text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
                                onClick={toggleActionMenu}
                              >
                                <DotsVerticalIcon className="w-4 h-4" />
                              </button>
                              <div className="js-inline-action-menu hidden absolute right-0 z-10 mt-1 w-32 bg-popover border border-border rounded-md shadow-lg">
                                <div className="py-1">
                                  <button
                                    onClick={(e: React.MouseEvent) => {
                                      e.stopPropagation();
                                      if (isArchivedSession) {
                                        return;
                                      }
                                      handleStartEdit(session.id, session.title);
                                      closeActionMenus();
                                    }}
                                    disabled={isArchivedSession}
                                    className={cn(
                                      'flex items-center w-full px-3 py-2 text-sm text-popover-foreground hover:bg-accent',
                                      isArchivedSession && 'opacity-50 cursor-not-allowed hover:bg-transparent',
                                    )}
                                  >
                                    <PencilIcon className="w-4 h-4 mr-2" />
                                    重命名
                                  </button>
                                  <button
                                    onClick={(e: React.MouseEvent) => {
                                      e.stopPropagation();
                                      handleDeleteSession(session.id);
                                      closeActionMenus();
                                    }}
                                    disabled={isArchivedSession}
                                    className={cn(
                                      'flex items-center w-full px-3 py-2 text-sm text-destructive hover:bg-destructive/10',
                                      isArchivedSession && 'opacity-50 cursor-not-allowed hover:bg-transparent',
                                    )}
                                  >
                                    <TrashIcon className="w-4 h-4 mr-2" />
                                    {isArchivedSession ? '已归档' : '归档'}
                                  </button>
                                </div>
                              </div>
                            </div>
                          )}
                        </div>
                      );
                    })}
                    {hasMore && (
                      <div className="pt-2">
                        <button
                          onClick={handleLoadMoreSessions}
                          disabled={isLoadingMore}
                          className="w-full px-3 py-2 text-sm text-muted-foreground hover:text-foreground border border-border rounded-lg hover:bg-accent transition-colors disabled:opacity-50"
                        >
                          {isLoadingMore ? '加载中...' : '加载更多'}
                        </button>
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}
          </div>

          <div className="my-2 border-t border-border" />

          <div className={cn('flex flex-col min-h-0', projectsExpanded ? 'flex-1' : 'shrink-0')}>
            <div className="px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
            <button
              type="button"
              onClick={() => {
                setProjectsExpanded((prev) => {
                  const next = !prev;
                  if (next) {
                    setSessionsExpanded(false);
                    setTerminalsExpanded(false);
                    setRemoteExpanded(false);
                  }
                  return next;
                });
              }}
              className="flex items-center gap-2 uppercase tracking-wide"
            >
              <span>{projectsExpanded ? '▾' : '▸'}</span>
              <span>PROJECTS</span>
            </button>
            <button
              type="button"
              onClick={openProjectModal}
              className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
              title="新增项目"
            >
              <PlusIcon className="w-4 h-4" />
            </button>
            </div>

            {projectsExpanded && (
              <div className="flex-1 min-h-0 overflow-y-auto">
                {projects.length === 0 ? (
                  <div className="px-3 py-3 text-xs text-muted-foreground">
                    还没有项目，点击右侧 + 新建。
                  </div>
                ) : (
                  <div className="p-2 space-y-1">
                    {projects.map((project: Project) => (
                      <div
                        key={project.id}
                        className={`group relative flex items-center p-2 rounded-lg cursor-pointer transition-colors ${
                          currentProject?.id === project.id
                            ? 'bg-accent border border-border'
                            : 'hover:bg-accent/50'
                        }`}
                        onClick={() => handleSelectProject(project.id)}
                      >
                        <div className="flex-1 min-w-0">
                          <h3 className="text-sm font-medium text-foreground truncate">
                            {project.name}
                          </h3>
                          <div className="mt-1 text-xs text-muted-foreground truncate" title={project.rootPath}>
                            {project.rootPath}
                          </div>
                        </div>
                        <div className="relative" data-action-menu-root="true">
                          <button
                            className="p-1 text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
                            onClick={toggleActionMenu}
                          >
                            <DotsVerticalIcon className="w-4 h-4" />
                          </button>
                          <div className="js-inline-action-menu hidden absolute right-0 z-10 mt-1 w-32 bg-popover border border-border rounded-md shadow-lg">
                            <div className="py-1">
                              <button
                                onClick={(e: React.MouseEvent) => {
                                  e.stopPropagation();
                                  handleDeleteProject(project.id);
                                  closeActionMenus();
                                }}
                                className="flex items-center w-full px-3 py-2 text-sm text-destructive hover:bg-destructive/10"
                              >
                                <TrashIcon className="w-4 h-4 mr-2" />
                                删除
                              </button>
                            </div>
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>

          <div className="my-2 border-t border-border" />

          <div className={cn('flex flex-col min-h-0', terminalsExpanded ? 'flex-1' : 'shrink-0')}>
            <div className="px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
              <button
                type="button"
                onClick={() => {
                  setTerminalsExpanded((prev) => {
                    const next = !prev;
                    if (next) {
                      setSessionsExpanded(false);
                      setProjectsExpanded(false);
                      setRemoteExpanded(false);
                    }
                    return next;
                  });
                }}
                className="flex items-center gap-2 uppercase tracking-wide"
              >
                <span>{terminalsExpanded ? '▾' : '▸'}</span>
                <span>TERMINALS</span>
              </button>
              <div className="flex items-center gap-1">
                <button
                  onClick={handleRefreshTerminals}
                  className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
                  title="刷新终端列表"
                >
                  <svg className={cn('w-4 h-4', isRefreshingTerminals && 'animate-spin')} fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12a7.5 7.5 0 0112.125-5.303M19.5 12a7.5 7.5 0 01-12.125 5.303M16.5 6.697V3m0 3.697h-3.697M7.5 17.303V21m0-3.697H3.803" />
                  </svg>
                </button>
                <button
                  type="button"
                  onClick={openTerminalModal}
                  className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
                  title="新增终端"
                >
                  <PlusIcon className="w-4 h-4" />
                </button>
              </div>
            </div>

            {terminalsExpanded && (
              <div className="flex-1 min-h-0 overflow-y-auto">
                {terminals.length === 0 ? (
                  <div className="px-3 py-3 text-xs text-muted-foreground">
                    还没有终端，点击右侧 + 新建。
                  </div>
                ) : (
                  <div className="p-2 space-y-1">
                    {terminals.map((terminal: Terminal) => (
                      <div
                        key={terminal.id}
                        className={`group relative flex items-center p-2 rounded-lg cursor-pointer transition-colors ${
                          currentTerminal?.id === terminal.id
                            ? 'bg-accent border border-border'
                            : 'hover:bg-accent/50'
                        }`}
                        onClick={() => handleSelectTerminal(terminal.id)}
                      >
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2">
                            <h3 className="text-sm font-medium text-foreground truncate">
                              {terminal.name}
                            </h3>
                            <span className={cn(
                              'inline-flex items-center text-[10px] px-1.5 py-0.5 rounded border',
                              terminal.status === 'running'
                                ? 'border-emerald-500/40 text-emerald-600'
                                : 'border-muted-foreground/40 text-muted-foreground'
                            )}>
                              {terminal.status === 'running' ? '运行中' : '已退出'}
                            </span>
                            {terminal.status === 'running' && (
                              <span className={cn(
                                'inline-flex items-center text-[10px] px-1.5 py-0.5 rounded border',
                                terminal.busy
                                  ? 'border-amber-500/40 text-amber-600'
                                  : 'border-emerald-500/30 text-emerald-600/80'
                              )}>
                                {terminal.busy ? '忙碌' : '空闲'}
                              </span>
                            )}
                          </div>
                          <div className="mt-1 text-xs text-muted-foreground truncate" title={terminal.cwd}>
                            {terminal.cwd}
                          </div>
                          {terminal.lastActiveAt && (
                            <div className="mt-1 text-[10px] text-muted-foreground/70">
                              最近活动：{formatTimeAgo(terminal.lastActiveAt)}
                            </div>
                          )}
                        </div>
                        <div className="relative" data-action-menu-root="true">
                          <button
                            className="p-1 text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
                            onClick={toggleActionMenu}
                          >
                            <DotsVerticalIcon className="w-4 h-4" />
                          </button>
                          <div className="js-inline-action-menu hidden absolute right-0 z-10 mt-1 w-32 bg-popover border border-border rounded-md shadow-lg">
                            <div className="py-1">
                              <button
                                onClick={(e: React.MouseEvent) => {
                                  e.stopPropagation();
                                  handleDeleteTerminal(terminal.id);
                                  closeActionMenus();
                                }}
                                className="flex items-center w-full px-3 py-2 text-sm text-destructive hover:bg-destructive/10"
                              >
                                <TrashIcon className="w-4 h-4 mr-2" />
                                删除
                              </button>
                            </div>
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>

          <div className="my-2 border-t border-border" />

          <div className={cn('flex flex-col min-h-0', remoteExpanded ? 'flex-1' : 'shrink-0')}>
            <div className="px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
              <button
                type="button"
                onClick={() => {
                  setRemoteExpanded((prev) => {
                    const next = !prev;
                    if (next) {
                      setSessionsExpanded(false);
                      setProjectsExpanded(false);
                      setTerminalsExpanded(false);
                    }
                    return next;
                  });
                }}
                className="flex items-center gap-2 uppercase tracking-wide"
              >
                <span>{remoteExpanded ? '▾' : '▸'}</span>
                <span>REMOTE</span>
              </button>
              <div className="flex items-center gap-1">
                <button
                  onClick={handleRefreshRemote}
                  className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
                  title="刷新远端连接列表"
                >
                  <svg className={cn('w-4 h-4', isRefreshingRemote && 'animate-spin')} fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12a7.5 7.5 0 0112.125-5.303M19.5 12a7.5 7.5 0 01-12.125 5.303M16.5 6.697V3m0 3.697h-3.697M7.5 17.303V21m0-3.697H3.803" />
                  </svg>
                </button>
                <button
                  type="button"
                  onClick={openRemoteModal}
                  className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
                  title="新增远端连接"
                >
                  <PlusIcon className="w-4 h-4" />
                </button>
              </div>
            </div>

            {remoteExpanded && (
              <div className="flex-1 min-h-0 overflow-y-auto">
                {remoteConnections.length === 0 ? (
                  <div className="px-3 py-3 text-xs text-muted-foreground">
                    还没有远端连接，点击右侧 + 新建。
                  </div>
                ) : (
                  <div className="p-2 space-y-1">
                    {remoteConnections.map((connection: RemoteConnection) => (
                      <div
                        key={connection.id}
                        className={`group relative flex items-center p-2 rounded-lg cursor-pointer transition-colors ${
                          currentRemoteConnection?.id === connection.id
                            ? 'bg-accent border border-border'
                            : 'hover:bg-accent/50'
                        }`}
                        onClick={() => handleSelectRemoteConnection(connection.id)}
                      >
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2">
                            <h3 className="text-sm font-medium text-foreground truncate">
                              {connection.name}
                            </h3>
                            <span className="inline-flex items-center text-[10px] px-1.5 py-0.5 rounded border border-blue-500/40 text-blue-600">
                              SSH
                            </span>
                          </div>
                          <div className="mt-1 text-xs text-muted-foreground truncate" title={`${connection.username}@${connection.host}:${connection.port}`}>
                            {connection.username}@{connection.host}:{connection.port}
                          </div>
                          {connection.lastActiveAt && (
                            <div className="mt-1 text-[10px] text-muted-foreground/70">
                              最近活动：{formatTimeAgo(connection.lastActiveAt)}
                            </div>
                          )}
                        </div>
                        <div className="flex items-center gap-1">
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              void handleOpenRemoteSftp(connection.id);
                            }}
                            className="rounded border border-border px-2 py-1 text-[10px] text-foreground hover:bg-accent"
                            title="打开 SFTP"
                          >
                            SFTP
                          </button>
                          <div className="relative" data-action-menu-root="true">
                            <button
                              className="p-1 text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
                              onClick={toggleActionMenu}
                            >
                              <DotsVerticalIcon className="w-4 h-4" />
                            </button>
                            <div className="js-inline-action-menu hidden absolute right-0 z-10 mt-1 w-36 bg-popover border border-border rounded-md shadow-lg">
                              <div className="py-1">
                                <button
                                  onClick={(e: React.MouseEvent) => {
                                    e.stopPropagation();
                                    openEditRemoteModal(connection);
                                    closeActionMenus();
                                  }}
                                  className="flex items-center w-full px-3 py-2 text-sm text-popover-foreground hover:bg-accent"
                                >
                                  <PencilIcon className="w-4 h-4 mr-2" />
                                  编辑
                                </button>
                                <button
                                  onClick={async (e: React.MouseEvent) => {
                                    e.stopPropagation();
                                    closeActionMenus();
                                    try {
                                      await apiClient.testRemoteConnection(connection.id);
                                      setRemoteSuccess(`连接测试成功 (${connection.name})`);
                                      setRemoteError(null);
                                    } catch (error) {
                                      setRemoteError(error instanceof Error ? error.message : '连接测试失败');
                                    }
                                  }}
                                  className="flex items-center w-full px-3 py-2 text-sm text-popover-foreground hover:bg-accent"
                                >
                                  <svg className="w-4 h-4 mr-2" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 12h16M12 4v16" />
                                  </svg>
                                  测试连接
                                </button>
                                <button
                                  onClick={(e: React.MouseEvent) => {
                                    e.stopPropagation();
                                    handleDeleteRemoteConnection(connection.id);
                                    closeActionMenus();
                                  }}
                                  className="flex items-center w-full px-3 py-2 text-sm text-destructive hover:bg-destructive/10"
                                >
                                  <TrashIcon className="w-4 h-4 mr-2" />
                                  删除
                                </button>
                              </div>
                            </div>
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
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
                  项目名称将默认使用：<span className="text-foreground">{deriveProjectName(projectRoot)}</span>
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
                  终端名称将默认使用：<span className="text-foreground">{deriveTerminalName(terminalRoot)}</span>
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
      {remoteModalOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="fixed inset-0 bg-black/50" onClick={() => setRemoteModalOpen(false)} />
          <div className="relative bg-card border border-border rounded-lg shadow-xl w-[620px] p-6 max-h-[85vh] overflow-y-auto">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold text-foreground">
                {editingRemoteConnectionId ? '编辑远端连接' : '新增远端连接'}
              </h3>
              <button
                onClick={() => setRemoteModalOpen(false)}
                className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
              >
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-3">
                <div className="col-span-2">
                  <label className="text-sm text-muted-foreground">名称（可选）</label>
                  <input
                    value={remoteName}
                    onChange={(e) => setRemoteName(e.target.value)}
                    className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="默认：user@host"
                  />
                </div>
                <div>
                  <label className="text-sm text-muted-foreground">主机</label>
                  <input
                    value={remoteHost}
                    onChange={(e) => setRemoteHost(e.target.value)}
                    className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="例如 1.2.3.4"
                  />
                </div>
                <div>
                  <label className="text-sm text-muted-foreground">端口</label>
                  <input
                    value={remotePort}
                    onChange={(e) => setRemotePort(e.target.value)}
                    className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="22"
                  />
                </div>
                <div>
                  <label className="text-sm text-muted-foreground">用户名</label>
                  <input
                    value={remoteUsername}
                    onChange={(e) => setRemoteUsername(e.target.value)}
                    className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="root"
                  />
                </div>
                <div>
                  <label className="text-sm text-muted-foreground">主机校验策略</label>
                  <select
                    value={remoteHostKeyPolicy}
                    onChange={(e) => setRemoteHostKeyPolicy(e.target.value as 'strict' | 'accept_new')}
                    className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                  >
                    <option value="strict">strict</option>
                    <option value="accept_new">accept_new</option>
                  </select>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="text-sm text-muted-foreground">认证方式</label>
                  <select
                    value={remoteAuthType}
                    onChange={(e) => setRemoteAuthType(e.target.value as 'private_key' | 'private_key_cert' | 'password')}
                    className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                  >
                    <option value="private_key">private_key</option>
                    <option value="private_key_cert">private_key_cert</option>
                    <option value="password">password</option>
                  </select>
                </div>
                <div>
                  <label className="text-sm text-muted-foreground">默认远端目录（可选）</label>
                  <input
                    value={remoteDefaultPath}
                    onChange={(e) => setRemoteDefaultPath(e.target.value)}
                    className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="例如 /home/root"
                  />
                </div>
                {remoteAuthType === 'password' ? (
                  <div className="col-span-2">
                    <label className="text-sm text-muted-foreground">密码</label>
                    <input
                      type="password"
                      value={remotePassword}
                      onChange={(e) => setRemotePassword(e.target.value)}
                      className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                      placeholder="请输入 SSH 登录密码"
                    />
                  </div>
                ) : (
                  <>
                    <div className="col-span-2">
                      <label className="text-sm text-muted-foreground">私钥路径</label>
                      <div className="mt-1 flex items-center gap-2">
                        <input
                          value={remotePrivateKeyPath}
                          onChange={(e) => setRemotePrivateKeyPath(e.target.value)}
                          className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                          placeholder="/Users/you/.ssh/id_rsa"
                        />
                        <button
                          type="button"
                          onClick={() => openKeyFilePicker('private_key')}
                          className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
                        >
                          选择文件
                        </button>
                      </div>
                    </div>
                    {remoteAuthType === 'private_key_cert' && (
                      <div className="col-span-2">
                        <label className="text-sm text-muted-foreground">证书路径</label>
                        <div className="mt-1 flex items-center gap-2">
                          <input
                            value={remoteCertificatePath}
                            onChange={(e) => setRemoteCertificatePath(e.target.value)}
                            className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                            placeholder="/Users/you/.ssh/id_rsa-cert.pub"
                          />
                          <button
                            type="button"
                            onClick={() => openKeyFilePicker('certificate')}
                            className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
                          >
                            选择文件
                          </button>
                        </div>
                      </div>
                    )}
                  </>
                )}
              </div>

              <div className="rounded border border-border p-3 space-y-3">
                <label className="inline-flex items-center gap-2 text-sm text-foreground">
                  <input
                    type="checkbox"
                    checked={remoteJumpEnabled}
                    onChange={(e) => setRemoteJumpEnabled(e.target.checked)}
                  />
                  启用跳板机
                </label>

                {remoteJumpEnabled && (
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="text-sm text-muted-foreground">跳板机主机</label>
                      <input
                        value={remoteJumpHost}
                        onChange={(e) => setRemoteJumpHost(e.target.value)}
                        className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                        placeholder="bastion.example.com"
                      />
                    </div>
                    <div>
                      <label className="text-sm text-muted-foreground">跳板机端口</label>
                      <input
                        value={remoteJumpPort}
                        onChange={(e) => setRemoteJumpPort(e.target.value)}
                        className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                        placeholder="22"
                      />
                    </div>
                    <div>
                      <label className="text-sm text-muted-foreground">跳板机用户名</label>
                      <input
                        value={remoteJumpUsername}
                        onChange={(e) => setRemoteJumpUsername(e.target.value)}
                        className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                        placeholder="jump_user"
                      />
                    </div>
                    <div>
                      <label className="text-sm text-muted-foreground">跳板机私钥路径（可选）</label>
                      <div className="mt-1 flex items-center gap-2">
                        <input
                          value={remoteJumpPrivateKeyPath}
                          onChange={(e) => setRemoteJumpPrivateKeyPath(e.target.value)}
                          className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                          placeholder="/Users/you/.ssh/jump_key"
                        />
                        <button
                          type="button"
                          onClick={() => openKeyFilePicker('jump_private_key')}
                          className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
                        >
                          选择文件
                        </button>
                      </div>
                    </div>
                    <div className="col-span-2">
                      <label className="text-sm text-muted-foreground">跳板机密码（可选）</label>
                      <input
                        type="password"
                        value={remoteJumpPassword}
                        onChange={(e) => setRemoteJumpPassword(e.target.value)}
                        className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                        placeholder="留空则尝试私钥/Agent/目标密码"
                      />
                    </div>
                  </div>
                )}
              </div>

              {remoteError && (
                <div className="text-xs text-destructive">{remoteError}</div>
              )}
              {remoteSuccess && (
                <div className="text-xs text-emerald-600">{remoteSuccess}</div>
              )}
            </div>
            <div className="mt-6 flex justify-end gap-2">
              <button
                onClick={() => setRemoteModalOpen(false)}
                className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
              >
                取消
              </button>
              <button
                onClick={handleTestRemoteConnection}
                disabled={remoteTesting || remoteSaving}
                className="px-4 py-2 rounded border border-border text-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {remoteTesting ? '测试中...' : '测试连接'}
              </button>
              <button
                onClick={handleSaveRemoteConnection}
                disabled={remoteSaving || remoteTesting}
                className="px-4 py-2 rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {remoteSaving ? '保存中...' : editingRemoteConnectionId ? '保存' : '创建'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 远端密钥文件选择弹窗 */}
      {keyFilePickerOpen && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center">
          <div className="fixed inset-0 bg-black/50" onClick={closeKeyFilePicker} />
          <div className="relative bg-card border border-border rounded-lg shadow-xl w-[680px] max-h-[82vh] p-6 flex flex-col">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-lg font-semibold text-foreground">{keyFilePickerTitle}</h3>
              <button
                onClick={closeKeyFilePicker}
                className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
              >
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div className="text-xs text-muted-foreground break-all">
              当前路径：<span className="text-foreground">{keyFilePickerPath || '请选择磁盘/目录'}</span>
            </div>
            <div className="mt-3 flex items-center gap-2">
              <button
                type="button"
                onClick={() => loadKeyFileEntries(keyFilePickerParent)}
                disabled={!keyFilePickerParent}
                className="px-3 py-1.5 rounded bg-muted text-muted-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                返回上级
              </button>
              <button
                type="button"
                onClick={() => loadKeyFileEntries(keyFilePickerPath)}
                className="px-3 py-1.5 rounded bg-muted text-muted-foreground hover:bg-accent"
              >
                刷新
              </button>
            </div>
            <div className="mt-3 flex-1 overflow-y-auto border border-border rounded">
              {keyFilePickerLoading && (
                <div className="p-4 text-sm text-muted-foreground">加载中...</div>
              )}
              {!keyFilePickerLoading && keyFilePickerItems.length === 0 && (
                <div className="p-4 text-sm text-muted-foreground">没有可用文件</div>
              )}
              {!keyFilePickerLoading && keyFilePickerItems.length > 0 && (
                <div className="divide-y divide-border">
                  {keyFilePickerItems.map((entry) => (
                    <div
                      key={entry.path}
                      className="px-4 py-2 hover:bg-accent flex items-center justify-between gap-3"
                    >
                      <button
                        type="button"
                        onClick={() => {
                          if (entry.isDir) {
                            loadKeyFileEntries(entry.path);
                          } else {
                            applySelectedKeyFile(entry.path);
                          }
                        }}
                        className="flex-1 text-left"
                      >
                        <span className="text-foreground truncate block">
                          {entry.isDir ? '📁' : '🔑'} {entry.name || entry.path}
                        </span>
                        <span className="text-[11px] text-muted-foreground truncate block">{entry.path}</span>
                      </button>
                      {!entry.isDir && (
                        <button
                          type="button"
                          onClick={() => applySelectedKeyFile(entry.path)}
                          className="px-2.5 py-1 rounded border border-border text-xs text-foreground hover:bg-accent"
                        >
                          选择
                        </button>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
            {keyFilePickerError && (
              <div className="mt-2 text-xs text-destructive">{keyFilePickerError}</div>
            )}
          </div>
        </div>
      )}

      {/* 目录选择弹窗 */}
      {dirPickerOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="fixed inset-0 bg-black/50" onClick={closeDirPicker} />
          <div className="relative bg-card border border-border rounded-lg shadow-xl w-[640px] max-h-[80vh] p-6 flex flex-col">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-lg font-semibold text-foreground">
                {dirPickerTarget === 'terminal' ? '选择终端目录' : '选择项目目录'}
              </h3>
              <button onClick={closeDirPicker} className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors">
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div className="text-xs text-muted-foreground break-all">
              当前路径：<span className="text-foreground">{dirPickerPath || '请选择盘符/目录'}</span>
            </div>
            <div className="mt-3 flex items-center gap-2">
              <button
                type="button"
                onClick={() => loadDirEntries(dirPickerParent)}
                disabled={!dirPickerParent}
                className="px-3 py-1.5 rounded bg-muted text-muted-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                返回上级
              </button>
              <button
                type="button"
                onClick={() => chooseDir(dirPickerPath)}
                disabled={!dirPickerPath}
                className="px-3 py-1.5 rounded bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                选择当前目录
              </button>
              {dirPickerTarget === 'project' && (
                <button
                  type="button"
                  onClick={openCreateDirModal}
                  disabled={!dirPickerPath || dirPickerCreatingFolder}
                  className="px-3 py-1.5 rounded bg-emerald-600 text-white hover:bg-emerald-700 disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {dirPickerCreatingFolder ? '新建中...' : '新建目录'}
                </button>
              )}
              {dirPickerTarget === 'project' && (
                <button
                  type="button"
                  onClick={() => setShowHiddenDirs((prev) => !prev)}
                  className="px-3 py-1.5 rounded bg-muted text-muted-foreground hover:bg-accent"
                >
                  {showHiddenDirs ? '不显示隐藏目录' : '显示隐藏目录'}
                </button>
              )}
            </div>
            <div className="mt-3 flex-1 overflow-y-auto border border-border rounded">
              {dirPickerLoading && (
                <div className="p-4 text-sm text-muted-foreground">加载中...</div>
              )}
              {!dirPickerLoading && dirPickerItems.length === 0 && (
                <div className="p-4 text-sm text-muted-foreground">没有可用目录</div>
              )}
              {!dirPickerLoading && dirPickerItems.length > 0 && (
                <div className="divide-y divide-border">
                  {dirPickerItems.map((entry) => (
                    <button
                      key={entry.path}
                      type="button"
                      onClick={() => loadDirEntries(entry.path)}
                      className="w-full text-left px-4 py-2 hover:bg-accent flex items-center gap-2"
                    >
                      <span className="text-foreground">{entry.name}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
            {dirPickerError && !dirPickerCreateModalOpen && (
              <div className="mt-2 text-xs text-red-500">{dirPickerError}</div>
            )}

            {dirPickerCreateModalOpen && (
              <div className="absolute inset-0 z-10 flex items-center justify-center">
                <div className="absolute inset-0 bg-black/40" onClick={() => !dirPickerCreatingFolder && setDirPickerCreateModalOpen(false)} />
                <div className="relative w-[420px] max-w-[90%] rounded-lg border border-border bg-card p-4 shadow-xl">
                  <div className="text-sm font-medium text-foreground mb-2">新建目录</div>
                  <div className="text-xs text-muted-foreground mb-3 break-all">
                    当前路径：<span className="text-foreground">{dirPickerPath || '-'}</span>
                  </div>
                  <input
                    autoFocus
                    value={dirPickerNewFolderName}
                    onChange={(e) => setDirPickerNewFolderName(e.target.value)}
                    placeholder="请输入新目录名称"
                    className="w-full px-3 py-2 rounded border border-border bg-background text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-ring"
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') {
                        e.preventDefault();
                        createDirInPicker();
                      } else if (e.key === 'Escape' && !dirPickerCreatingFolder) {
                        e.preventDefault();
                        setDirPickerCreateModalOpen(false);
                      }
                    }}
                  />
                  {dirPickerError && (
                    <div className="mt-2 text-xs text-red-500">{dirPickerError}</div>
                  )}
                  <div className="mt-4 flex justify-end gap-2">
                    <button
                      type="button"
                      onClick={() => setDirPickerCreateModalOpen(false)}
                      disabled={dirPickerCreatingFolder}
                      className="px-3 py-1.5 rounded bg-muted text-muted-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      取消
                    </button>
                    <button
                      type="button"
                      onClick={createDirInPicker}
                      disabled={dirPickerCreatingFolder}
                      className="px-3 py-1.5 rounded bg-emerald-600 text-white hover:bg-emerald-700 disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {dirPickerCreatingFolder ? '新建中...' : '确定'}
                    </button>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

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
