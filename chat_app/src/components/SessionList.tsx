import React, { useCallback, useMemo, useState, useEffect, useRef } from 'react';
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
import { CreateContactModal } from './sessionList/CreateContactModal';
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
import type {
  DirPickerTarget,
  KeyFilePickerTarget,
} from './sessionList/helpers';
import {
  mergeSessionRuntimeIntoMetadata,
  readSessionRuntimeFromMetadata,
} from '../lib/store/helpers/sessionRuntime';

const resolveContactAgentIdFromSession = (session: Session | null | undefined): string | null => {
  if (!session) {
    return null;
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  if (!runtime?.contactAgentId) {
    return null;
  }
  const trimmed = runtime.contactAgentId.trim();
  return trimmed.length > 0 ? trimmed : null;
};

const resolveContactIdFromSession = (session: Session | null | undefined): string | null => {
  if (!session) {
    return null;
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  if (!runtime?.contactId) {
    return null;
  }
  const trimmed = runtime.contactId.trim();
  return trimmed.length > 0 ? trimmed : null;
};

const resolveSessionProjectId = (session: Session | Record<string, any> | null | undefined): string | null => {
  if (!session) {
    return null;
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  const runtimeProjectId = typeof runtime?.projectId === 'string'
    ? runtime.projectId.trim()
    : '';
  if (runtimeProjectId) {
    return runtimeProjectId;
  }
  const rawProjectId = typeof (session as any).projectId === 'string'
    ? (session as any).projectId.trim()
    : (typeof (session as any).project_id === 'string'
      ? (session as any).project_id.trim()
      : '');
  return rawProjectId || null;
};

const resolveSessionTimestamp = (session: Session | Record<string, any> | null | undefined): number => {
  if (!session) {
    return 0;
  }
  const raw = (session as any).updatedAt
    ?? (session as any).updated_at
    ?? (session as any).createdAt
    ?? (session as any).created_at
    ?? Date.now();
  const ts = new Date(raw).getTime();
  return Number.isFinite(ts) ? ts : 0;
};

const isSessionActive = (session: Session | Record<string, any> | null | undefined): boolean => {
  if (!session) {
    return false;
  }
  const archived = (session as any).archived === true;
  const status = typeof (session as any).status === 'string'
    ? (session as any).status.toLowerCase()
    : '';
  return !archived && status !== 'archived' && status !== 'archiving';
};

const isSessionMatchedContactAndProject = (
  session: Session | Record<string, any> | null | undefined,
  contact: { id: string; agentId: string },
  projectId: string | null,
): boolean => {
  if (!session || !isSessionActive(session)) {
    return false;
  }

  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  const contactId = typeof runtime?.contactId === 'string' ? runtime.contactId.trim() : '';
  const contactAgentId = typeof runtime?.contactAgentId === 'string' ? runtime.contactAgentId.trim() : '';

  if (contactId) {
    if (contactId !== contact.id) {
      return false;
    }
  } else if (contactAgentId) {
    if (contactAgentId !== contact.agentId) {
      return false;
    }
  } else {
    return false;
  }

  const normalizedProjectId = typeof projectId === 'string' ? projectId.trim() : '';
  if (!normalizedProjectId) {
    return true;
  }
  const sessionProjectId = resolveSessionProjectId(session);
  return sessionProjectId === normalizedProjectId;
};

type ContactItem = {
  id: string;
  agentId: string;
  name: string;
  status: string;
  createdAt: Date;
  updatedAt: Date;
};

interface SessionListProps {
  isOpen?: boolean;
  onClose?: () => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
  className?: string;
  store?: typeof useChatStore;
  onSelectSession?: (sessionId: string) => void;
}

export const SessionList: React.FC<SessionListProps> = (props) => {
  const {
    isOpen = true,
    collapsed,
    className,
    store,
    onSelectSession,
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
    contacts,
    agents,
    currentSession,
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
  } = storeToUse((state) => ({
    sessions: state.sessions,
    contacts: state.contacts,
    agents: state.agents,
    currentSession: state.currentSession,
    loadContacts: state.loadContacts,
    createContact: state.createContact,
    deleteContact: state.deleteContact,
    createSession: state.createSession,
    selectSession: state.selectSession,
    deleteSession: state.deleteSession,
    updateSession: state.updateSession,
    loadAgents: state.loadAgents,
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
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [sessionsExpanded, setSessionsExpanded] = useState(true);
  const [projectsExpanded, setProjectsExpanded] = useState(true);
  const [terminalsExpanded, setTerminalsExpanded] = useState(true);
  const [remoteExpanded, setRemoteExpanded] = useState(true);
  const [isRefreshingTerminals, setIsRefreshingTerminals] = useState(false);
  const [isRefreshingRemote, setIsRefreshingRemote] = useState(false);
  const contactSessionCacheRef = useRef<Record<string, string>>({});

  const [createContactModalOpen, setCreateContactModalOpen] = useState(false);
  const [selectedContactAgentId, setSelectedContactAgentId] = useState<string | null>(null);
  const [contactError, setContactError] = useState<string | null>(null);

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

  const didLoadProjectsRef = useRef(false);
  const didLoadAgentsRef = useRef(false);
  const didLoadContactsRef = useRef(false);
  const didLoadTerminalsRef = useRef(false);
  const didLoadRemoteRef = useRef(false);
  
  const { dialogState, showConfirmDialog, handleConfirm, handleCancel } = useConfirmDialog();

  const isCollapsed = collapsed ?? !isOpen;
  const existingContactAgentIds = useMemo(
    () => (contacts || []).map((item: ContactItem) => item.agentId),
    [contacts],
  );

  const resolveContactProjectKey = useCallback((contact: ContactItem): string => {
    const projectId = currentProject?.id?.trim() || '';
    return `${contact.id}::${projectId}`;
  }, [currentProject?.id]);

  const findExistingSessionIdInStore = useCallback((contact: ContactItem): string | null => {
    const projectId = currentProject?.id?.trim() || null;
    const candidates = (sessions || []).filter((session: Session) =>
      isSessionMatchedContactAndProject(session, contact, projectId),
    );
    if (candidates.length === 0) {
      return null;
    }
    candidates.sort((a, b) => resolveSessionTimestamp(b) - resolveSessionTimestamp(a));
    const id = typeof candidates[0]?.id === 'string' ? candidates[0].id.trim() : '';
    return id || null;
  }, [currentProject?.id, sessions]);

  const findExistingSessionIdFromApi = useCallback(async (contact: ContactItem): Promise<string | null> => {
    const projectId = currentProject?.id?.trim() || null;
    const pageSize = 200;
    const maxPages = 8;
    const candidates: any[] = [];

    for (let page = 0; page < maxPages; page += 1) {
      const rows = await apiClient.getSessions(undefined, undefined, {
        limit: pageSize,
        offset: page * pageSize,
      });
      if (!Array.isArray(rows) || rows.length === 0) {
        break;
      }

      for (const row of rows) {
        if (isSessionMatchedContactAndProject(row, contact, projectId)) {
          candidates.push(row);
        }
      }

      if (rows.length < pageSize) {
        break;
      }
    }

    if (candidates.length === 0) {
      return null;
    }

    candidates.sort((a, b) => resolveSessionTimestamp(b) - resolveSessionTimestamp(a));
    const shortlist = candidates.slice(0, 20);
    for (const item of shortlist) {
      const sessionId = typeof item?.id === 'string' ? item.id.trim() : '';
      if (!sessionId) {
        continue;
      }
      try {
        const previewMessages = await apiClient.getSessionMessages(sessionId, {
          limit: 1,
          offset: 0,
          compact: false,
        });
        if (Array.isArray(previewMessages) && previewMessages.length > 0) {
          return sessionId;
        }
      } catch {
        // ignore preview errors and keep fallback strategy
      }
    }

    const fallback = shortlist.find((item) => typeof item?.id === 'string' && item.id.trim());
    return fallback ? fallback.id.trim() : null;
  }, [apiClient, currentProject?.id]);

  const ensureSessionForContact = useCallback(async (contact: ContactItem): Promise<string | null> => {
    const cacheKey = resolveContactProjectKey(contact);
    const cachedSessionId = contactSessionCacheRef.current[cacheKey];
    if (cachedSessionId && cachedSessionId.trim()) {
      return cachedSessionId.trim();
    }

    const currentContactId = resolveContactIdFromSession(currentSession);
    const currentContactAgentId = resolveContactAgentIdFromSession(currentSession);
    if (
      currentSession?.id
      && (
        (currentContactId && currentContactId === contact.id)
        || (currentContactAgentId && currentContactAgentId === contact.agentId)
      )
    ) {
      contactSessionCacheRef.current[cacheKey] = currentSession.id;
      return currentSession.id;
    }

    const existingLocalSessionId = findExistingSessionIdInStore(contact);
    if (existingLocalSessionId) {
      contactSessionCacheRef.current[cacheKey] = existingLocalSessionId;
      return existingLocalSessionId;
    }

    try {
      const existingSessionId = await findExistingSessionIdFromApi(contact);
      if (existingSessionId) {
        contactSessionCacheRef.current[cacheKey] = existingSessionId;
        return existingSessionId;
      }
    } catch (error) {
      console.error('Failed to resolve existing contact session:', error);
    }

    const createdSessionId = await createSession({
      title: contact.name || '联系人',
      contactAgentId: contact.agentId,
      contactId: contact.id,
      selectedModelId: null,
      projectId: currentProject?.id || null,
      projectRoot: currentProject?.rootPath || null,
      mcpEnabled: true,
      enabledMcpIds: [],
    });
    if (createdSessionId) {
      contactSessionCacheRef.current[cacheKey] = createdSessionId;
    }
    return createdSessionId;
  }, [
    resolveContactProjectKey,
    currentSession,
    createSession,
    currentProject,
    findExistingSessionIdInStore,
    findExistingSessionIdFromApi,
  ]);

  const displaySessions = useMemo<Session[]>(() => {
    return contacts.map((contact) => {
      return {
        id: `contact-placeholder:${contact.id}`,
        title: contact.name,
        createdAt: contact.createdAt,
        updatedAt: contact.updatedAt,
        messageCount: 0,
        tokenUsage: 0,
        pinned: false,
        archived: false,
        status: 'active',
        metadata: mergeSessionRuntimeIntoMetadata(null, {
          contactAgentId: contact.agentId,
          contactId: contact.id,
          selectedModelId: null,
          projectId: currentProject?.id || null,
          projectRoot: currentProject?.rootPath || null,
          mcpEnabled: true,
          enabledMcpIds: [],
        }),
      } as Session;
    });
  }, [contacts, currentProject]);

  const currentDisplaySessionId = useMemo(() => {
    const currentContactId = resolveContactIdFromSession(currentSession);
    if (currentContactId) {
      return `contact-placeholder:${currentContactId}`;
    }

    const currentContactAgentId = resolveContactAgentIdFromSession(currentSession);
    if (!currentContactAgentId) {
      return null;
    }
    const matched = contacts.find((item) => item.agentId === currentContactAgentId);
    if (!matched) {
      return null;
    }
    return `contact-placeholder:${matched.id}`;
  }, [contacts, currentSession]);

  const handleCreateSession = async () => {
    setContactError(null);
    setSelectedContactAgentId(null);
    try {
      await loadContactsAction();
    } catch (error) {
      setContactError(error instanceof Error ? error.message : '加载联系人失败');
    }
    setCreateContactModalOpen(true);
  };

  const handleCreateContactSession = async () => {
    const agentId = selectedContactAgentId?.trim();
    if (!agentId) {
      setContactError('请先选择一个联系人');
      return;
    }
    const selectedAgent = (agents || []).find((agent: any) => agent.id === agentId);
    if (!selectedAgent) {
      setContactError('联系人不存在或不可用');
      return;
    }
    try {
      const createdContact = await createContactAction(
        selectedAgent.id,
        selectedAgent.name || undefined,
      );
      const matchedContact: ContactItem = {
        id: createdContact.id,
        agentId: createdContact.agentId,
        name: createdContact.name,
        status: createdContact.status,
        createdAt: createdContact.createdAt,
        updatedAt: createdContact.updatedAt,
      };
      const ensuredSessionId = await ensureSessionForContact(matchedContact);
      if (ensuredSessionId) {
        const metadata = mergeSessionRuntimeIntoMetadata(null, {
          contactAgentId: selectedAgent.id,
          contactId: createdContact.id || null,
          selectedModelId: null,
          projectId: currentProject?.id || null,
          projectRoot: currentProject?.rootPath || null,
          mcpEnabled: true,
          enabledMcpIds: [],
        });
        await updateSession(ensuredSessionId, { metadata } as Partial<Session>);
        if (currentSession?.id !== ensuredSessionId) {
          await selectSession(ensuredSessionId);
        }
      }

      await loadContactsAction();

      setCreateContactModalOpen(false);
      setSelectedContactAgentId(null);
      setContactError(null);
    } catch (error) {
      console.error('Failed to create session:', error);
      setContactError(error instanceof Error ? error.message : '添加联系人失败');
    }
  };

  const handleSelectSession = async (sessionId: string): Promise<string | null> => {
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
          await selectSession(ensuredSessionId);
        }
        return ensuredSessionId;
      }
      if (currentSession?.id !== sessionId) {
        await selectSession(sessionId);
      }
      return sessionId;
    } catch (error) {
      console.error('Failed to select session:', error);
      return null;
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
    await loadContactsAction();
    setIsRefreshing(false);
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
    const session = displaySessions.find((s: Session) => s.id === sessionId);
    if (!session || getSessionStatus(session) !== 'active') {
      return;
    }
    const runtime = readSessionRuntimeFromMetadata(session.metadata);
    const contactAgentId = runtime?.contactAgentId || null;
    showConfirmDialog({
      title: '删除联系人',
      message: `确定要删除联系人 "${session.title || 'Untitled'}" 吗？`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          let resolvedContactId = runtime?.contactId || null;
          if (!resolvedContactId && contactAgentId) {
            const matched = contacts.find((item: ContactItem) => item.agentId === contactAgentId) || null;
            resolvedContactId = matched?.id || null;
          }
          if (resolvedContactId) {
            await deleteContactAction(resolvedContactId);
            const prefix = `${resolvedContactId}::`;
            for (const [key, cachedSessionId] of Object.entries(contactSessionCacheRef.current)) {
              if (!key.startsWith(prefix)) {
                continue;
              }
              delete contactSessionCacheRef.current[key];
              if (cachedSessionId && currentSession?.id === cachedSessionId) {
                await deleteSession(cachedSessionId);
              }
            }
          }
          if (!sessionId.startsWith('contact-placeholder:')) {
            await deleteSession(sessionId);
          }
          if (resolvedContactId) {
            // ensure local contact state is fresh for edge cases (cross-tab changes)
            await loadContactsAction();
          }
        } catch (error) {
          console.error('Failed to delete session:', error);
        }
      }
    });
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
    if (didLoadProjectsRef.current) return;
    didLoadProjectsRef.current = true;
    loadProjects();
  }, [loadProjects]);

  useEffect(() => {
    if (didLoadAgentsRef.current) return;
    didLoadAgentsRef.current = true;
    loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    if (didLoadContactsRef.current) return;
    didLoadContactsRef.current = true;
    loadContactsAction().catch((error) => {
      console.error('Failed to load contacts:', error);
    });
  }, [loadContactsAction]);

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
      {/* 联系人与项目列表 */}
      {!isCollapsed && (
        <div className="flex-1 flex flex-col overflow-hidden">
          <SessionSection
            expanded={sessionsExpanded}
            sessions={displaySessions}
            currentSessionId={currentDisplaySessionId}
            sessionChatState={sessionChatState}
            taskReviewPanelsBySession={taskReviewPanelsBySession}
            uiPromptPanelsBySession={uiPromptPanelsBySession}
            hasMore={false}
            isRefreshing={isRefreshing}
            isLoadingMore={false}
            onToggle={handleToggleSessionsSection}
            onRefresh={handleRefreshSessions}
            onCreateSession={handleCreateSession}
            onSelectSession={(sessionId) => {
              void handleSelectSession(sessionId).then((selectedSessionId) => {
                if (selectedSessionId) {
                  onSelectSession?.(selectedSessionId);
                }
              });
            }}
            onDeleteSession={handleDeleteSession}
            onLoadMore={() => {}}
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

      <CreateContactModal
        isOpen={createContactModalOpen}
        agents={(agents || []) as any[]}
        existingAgentIds={existingContactAgentIds}
        selectedAgentId={selectedContactAgentId}
        error={contactError}
        onClose={() => {
          setCreateContactModalOpen(false);
          setSelectedContactAgentId(null);
          setContactError(null);
        }}
        onSelectedAgentChange={(agentId) => {
          setSelectedContactAgentId(agentId);
          setContactError(null);
        }}
        onCreate={() => {
          void handleCreateContactSession();
        }}
      />

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
