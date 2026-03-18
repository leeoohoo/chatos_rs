import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { shallow } from 'zustand/shallow';

import { InputArea } from '../InputArea';
import { MessageList } from '../MessageList';
import { ProjectContactPickerModal } from '../sessionList/ProjectContactPickerModal';
import { apiClient as globalApiClient } from '../../lib/api/client';
import { useChatApiClientFromContext, useChatStoreSelector } from '../../lib/store/ChatStoreContext';
import {
  mergeSessionRuntimeIntoMetadata,
  readSessionRuntimeFromMetadata,
} from '../../lib/store/helpers/sessionRuntime';
import { cn } from '../../lib/utils';
import type { Project, Session } from '../../types';

type ContactItem = {
  id: string;
  agentId: string;
  name: string;
};

type ProjectContactLink = {
  contactId: string;
  agentId: string;
  name: string;
  updatedAt: number;
};

type SessionSummaryItem = {
  id: string;
  summaryText: string;
  summaryModel: string;
  triggerType: string;
  sourceMessageCount: number;
  sourceEstimatedTokens: number;
  status: string;
  errorMessage: string | null;
  level: number;
  createdAt: string;
  updatedAt: string;
};

interface TeamMembersPaneProps {
  project: Project;
  className?: string;
}

const normalizeProjectScopeId = (projectId: string | null | undefined): string => {
  const trimmed = typeof projectId === 'string' ? projectId.trim() : '';
  return trimmed.length > 0 ? trimmed : '0';
};

const resolveSessionProjectScopeId = (session: Session | Record<string, any> | null | undefined): string => {
  if (!session) {
    return '0';
  }
  const rawProjectId = typeof (session as any).projectId === 'string'
    ? (session as any).projectId.trim()
    : (typeof (session as any).project_id === 'string'
      ? (session as any).project_id.trim()
      : '');
  if (rawProjectId.length > 0) {
    return normalizeProjectScopeId(rawProjectId);
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  return normalizeProjectScopeId(runtime?.projectId ?? null);
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

const formatSummaryTime = (value?: string | null): string => {
  if (!value) {
    return '-';
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return parsed.toLocaleString();
};

const normalizeSessionSummary = (item: any): SessionSummaryItem | null => {
  const id = typeof item?.id === 'string' ? item.id.trim() : '';
  if (!id) {
    return null;
  }
  const createdAt = typeof item?.created_at === 'string'
    ? item.created_at
    : (typeof item?.createdAt === 'string' ? item.createdAt : '');
  const updatedAt = typeof item?.updated_at === 'string'
    ? item.updated_at
    : (typeof item?.updatedAt === 'string' ? item.updatedAt : createdAt);

  return {
    id,
    summaryText: typeof item?.summary_text === 'string'
      ? item.summary_text
      : (typeof item?.summaryText === 'string' ? item.summaryText : ''),
    summaryModel: typeof item?.summary_model === 'string'
      ? item.summary_model
      : (typeof item?.summaryModel === 'string' ? item.summaryModel : ''),
    triggerType: typeof item?.trigger_type === 'string'
      ? item.trigger_type
      : (typeof item?.triggerType === 'string' ? item.triggerType : ''),
    sourceMessageCount: Number(item?.source_message_count ?? item?.sourceMessageCount ?? 0) || 0,
    sourceEstimatedTokens: Number(item?.source_estimated_tokens ?? item?.sourceEstimatedTokens ?? 0) || 0,
    status: typeof item?.status === 'string' ? item.status : '',
    errorMessage: typeof item?.error_message === 'string'
      ? item.error_message
      : (typeof item?.errorMessage === 'string' ? item.errorMessage : null),
    level: Number(item?.level ?? 0) || 0,
    createdAt,
    updatedAt,
  };
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

const matchContactSession = (session: Session, contact: ContactItem, projectId: string): boolean => {
  if (!isSessionActive(session)) {
    return false;
  }
  if (resolveSessionProjectScopeId(session) !== projectId) {
    return false;
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  const contactId = typeof runtime?.contactId === 'string' ? runtime.contactId.trim() : '';
  const contactAgentId = typeof runtime?.contactAgentId === 'string' ? runtime.contactAgentId.trim() : '';
  if (contactId) {
    return contactId === contact.id;
  }
  if (contactAgentId) {
    return contactAgentId === contact.agentId;
  }
  return false;
};

const TeamMembersPane: React.FC<TeamMembersPaneProps> = ({ project, className }) => {
  const {
    currentSession,
    sessions,
    contacts,
    loadContacts,
    messages,
    hasMoreMessages,
    sessionChatState,
    sendMessage,
    abortCurrentConversation,
    loadMoreMessages,
    toggleTurnProcess,
    createSession,
    selectSession,
    updateSession,
    aiModelConfigs,
    selectedModelId,
    setSelectedModel,
    chatConfig,
    updateChatConfig,
  } = useChatStoreSelector((state) => ({
    currentSession: state.currentSession,
    sessions: state.sessions,
    contacts: state.contacts,
    loadContacts: state.loadContacts,
    messages: state.messages,
    hasMoreMessages: state.hasMoreMessages,
    sessionChatState: state.sessionChatState,
    sendMessage: state.sendMessage,
    abortCurrentConversation: state.abortCurrentConversation,
    loadMoreMessages: state.loadMoreMessages,
    toggleTurnProcess: state.toggleTurnProcess,
    createSession: state.createSession,
    selectSession: state.selectSession,
    updateSession: state.updateSession,
    aiModelConfigs: state.aiModelConfigs,
    selectedModelId: state.selectedModelId,
    setSelectedModel: state.setSelectedModel,
    chatConfig: state.chatConfig,
    updateChatConfig: state.updateChatConfig,
  }), shallow);
  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = useMemo(
    () => apiClientFromContext || globalApiClient,
    [apiClientFromContext],
  );

  const [selectedContactId, setSelectedContactId] = useState<string | null>(null);
  const [switchingContactId, setSwitchingContactId] = useState<string | null>(null);
  const [composerMcpEnabled, setComposerMcpEnabled] = useState(true);
  const [composerEnabledMcpIds, setComposerEnabledMcpIds] = useState<string[]>([]);
  const [projectMembers, setProjectMembers] = useState<ProjectContactLink[]>([]);
  const [projectMembersLoading, setProjectMembersLoading] = useState(false);
  const [projectMembersReloadSeed, setProjectMembersReloadSeed] = useState(0);
  const [projectMembersError, setProjectMembersError] = useState<string | null>(null);
  const [memberPickerOpen, setMemberPickerOpen] = useState(false);
  const [memberPickerSelectedId, setMemberPickerSelectedId] = useState<string | null>(null);
  const [memberPickerError, setMemberPickerError] = useState<string | null>(null);
  const [removingContactId, setRemovingContactId] = useState<string | null>(null);
  const [summaryPaneSessionId, setSummaryPaneSessionId] = useState<string | null>(null);
  const [summaryItems, setSummaryItems] = useState<SessionSummaryItem[]>([]);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [clearingSummaries, setClearingSummaries] = useState(false);
  const [deletingSummaryId, setDeletingSummaryId] = useState<string | null>(null);
  const [openingSummaryContactId, setOpeningSummaryContactId] = useState<string | null>(null);

  const normalizedProjectId = normalizeProjectScopeId(project?.id || null);
  const emitProjectContactChanged = useCallback((projectId: string) => {
    if (typeof window === 'undefined') {
      return;
    }
    window.dispatchEvent(new CustomEvent('project-contact-changed', {
      detail: { projectId },
    }));
  }, []);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    const handler = (event: Event) => {
      const customEvent = event as CustomEvent<{ projectId?: string }>;
      const changedProjectId = normalizeProjectScopeId(customEvent?.detail?.projectId ?? null);
      if (changedProjectId !== normalizedProjectId) {
        return;
      }
      setProjectMembersReloadSeed((prev) => prev + 1);
    };
    window.addEventListener('project-contact-changed', handler as EventListener);
    return () => {
      window.removeEventListener('project-contact-changed', handler as EventListener);
    };
  }, [normalizedProjectId]);

  useEffect(() => {
    let cancelled = false;
    const loadProjectMembers = async () => {
      if (!project?.id) {
        setProjectMembers([]);
        setProjectMembersLoading(false);
        return;
      }
      setProjectMembersLoading(true);
      setProjectMembersError(null);
      try {
        const rows = await apiClient.listProjectContacts(project.id, { limit: 500, offset: 0 });
        if (cancelled) {
          return;
        }
        const dedupedByContact = new Map<string, ProjectContactLink>();
        for (const item of (Array.isArray(rows) ? rows : [])) {
          const contactId = typeof item?.contact_id === 'string' ? item.contact_id.trim() : '';
          const agentId = typeof item?.agent_id === 'string' ? item.agent_id.trim() : '';
          if (!contactId || !agentId) {
            continue;
          }
          const name = typeof item?.agent_name_snapshot === 'string' && item.agent_name_snapshot.trim()
            ? item.agent_name_snapshot.trim()
            : contactId;
          const ts = new Date(
            typeof item?.updated_at === 'string' && item.updated_at
              ? item.updated_at
              : (typeof item?.last_bound_at === 'string' ? item.last_bound_at : Date.now()),
          ).getTime();
          const normalizedUpdatedAt = Number.isFinite(ts) ? ts : 0;
          const existing = dedupedByContact.get(contactId);
          if (!existing || normalizedUpdatedAt >= existing.updatedAt) {
            dedupedByContact.set(contactId, {
              contactId,
              agentId,
              name,
              updatedAt: normalizedUpdatedAt,
            });
          }
        }
        const nextMembers = Array.from(dedupedByContact.values())
          .sort((left, right) => right.updatedAt - left.updatedAt);
        setProjectMembers(nextMembers);
      } catch (error) {
        if (!cancelled) {
          setProjectMembersError(error instanceof Error ? error.message : '加载项目成员失败');
          setProjectMembers([]);
        }
      } finally {
        if (!cancelled) {
          setProjectMembersLoading(false);
        }
      }
    };
    void loadProjectMembers();
    return () => {
      cancelled = true;
    };
  }, [apiClient, project?.id, projectMembersReloadSeed]);

  const findProjectSessionForContact = useCallback((contact: ContactItem): Session | null => {
    const candidates = (sessions || []).filter((session: Session) =>
      matchContactSession(session, contact, normalizedProjectId),
    );
    if (candidates.length === 0) {
      return null;
    }
    candidates.sort((a, b) => resolveSessionTimestamp(b) - resolveSessionTimestamp(a));
    return candidates[0] || null;
  }, [normalizedProjectId, sessions]);

  const projectContacts = useMemo(() => {
    const rows = projectMembers.map((member) => {
      const contact: ContactItem = {
        id: member.contactId,
        agentId: member.agentId,
        name: member.name,
      };
        const session = findProjectSessionForContact(contact);
        return {
          contact,
          session,
          updatedAt: session ? resolveSessionTimestamp(session) : member.updatedAt,
        };
      });
    rows.sort((a, b) => b.updatedAt - a.updatedAt);
    return rows;
  }, [findProjectSessionForContact, projectMembers]);
  const projectContactIdSet = useMemo(
    () => new Set(projectMembers.map((item) => item.contactId)),
    [projectMembers],
  );
  const projectContactsOptions = useMemo(() => (
    (contacts || []).map((item: any) => ({
      id: item.id,
      name: item.name,
      agentId: item.agentId,
    }))
  ), [contacts]);

  const selectedContact = useMemo(() => {
    if (!selectedContactId) {
      return null;
    }
    const matched = projectContacts.find((item) => item.contact.id === selectedContactId);
    return matched?.contact || null;
  }, [projectContacts, selectedContactId]);

  const selectedProjectSession = useMemo(() => {
    if (!selectedContact) {
      return null;
    }
    return findProjectSessionForContact(selectedContact);
  }, [findProjectSessionForContact, selectedContact]);

  const isSelectedSessionActive = Boolean(
    selectedProjectSession?.id
    && currentSession?.id
    && selectedProjectSession.id === currentSession.id,
  );
  const sessionSummaryPaneVisible = Boolean(
    selectedProjectSession?.id
    && summaryPaneSessionId
    && selectedProjectSession.id === summaryPaneSessionId,
  );

  useEffect(() => {
    const runtime = readSessionRuntimeFromMetadata(currentSession?.metadata);
    setComposerMcpEnabled(runtime?.mcpEnabled ?? true);
    setComposerEnabledMcpIds(runtime?.enabledMcpIds ?? []);
  }, [currentSession?.id, currentSession?.metadata]);

  const ensureContactSession = useCallback(async (contact: ContactItem): Promise<string | null> => {
    const existing = findProjectSessionForContact(contact);
    if (existing?.id) {
      if (currentSession?.id !== existing.id) {
        await selectSession(existing.id, { keepActivePanel: true });
      }
      return existing.id;
    }

    const createdId = await createSession({
      title: contact.name || '联系人',
      contactAgentId: contact.agentId,
      contactId: contact.id,
      selectedModelId: selectedModelId ?? null,
      projectId: normalizedProjectId,
      projectRoot: project.rootPath || null,
      mcpEnabled: true,
      enabledMcpIds: [],
    }, {
      keepActivePanel: true,
    });

    if (createdId && currentSession?.id !== createdId) {
      await selectSession(createdId, { keepActivePanel: true });
    }
    return createdId || null;
  }, [
    createSession,
    currentSession?.id,
    findProjectSessionForContact,
    normalizedProjectId,
    project.rootPath,
    selectSession,
    selectedModelId,
  ]);

  const handleSelectContact = useCallback(async (contactId: string) => {
    const contact = projectContacts.find((item) => item.contact.id === contactId)?.contact || null;
    if (!contact) {
      return;
    }
    setSelectedContactId(contactId);
    setSwitchingContactId(contactId);
    try {
      const sessionId = await ensureContactSession(contact);
      if (summaryPaneSessionId && sessionId && sessionId !== summaryPaneSessionId) {
        setSummaryPaneSessionId(null);
        setSummaryItems([]);
        setSummaryError(null);
      }
    } finally {
      setSwitchingContactId((prev) => (prev === contactId ? null : prev));
    }
  }, [ensureContactSession, projectContacts, summaryPaneSessionId]);

  useEffect(() => {
    if (projectContacts.length === 0) {
      setSelectedContactId(null);
      return;
    }
    if (selectedContactId && projectContacts.some((item) => item.contact.id === selectedContactId)) {
      return;
    }
    const firstId = projectContacts[0].contact.id;
    void handleSelectContact(firstId);
  }, [handleSelectContact, projectContacts, selectedContactId]);

  const selectedSessionChatState = useMemo(() => {
    if (!selectedProjectSession?.id) {
      return undefined;
    }
    return sessionChatState[selectedProjectSession.id];
  }, [selectedProjectSession?.id, sessionChatState]);

  const chatIsLoading = selectedSessionChatState?.isLoading ?? false;
  const chatIsStreaming = selectedSessionChatState?.isStreaming ?? false;
  const chatIsStopping = selectedSessionChatState?.isStopping ?? false;

  const supportsReasoning = useMemo(() => {
    if (!selectedModelId) {
      return false;
    }
    const matched = (aiModelConfigs || []).find((item: any) => item.id === selectedModelId);
    return matched?.supports_reasoning === true;
  }, [aiModelConfigs, selectedModelId]);

  const handleLoadMore = useCallback(() => {
    if (selectedProjectSession?.id) {
      loadMoreMessages(selectedProjectSession.id);
    }
  }, [loadMoreMessages, selectedProjectSession?.id]);

  const handleToggleTurnProcess = useCallback((userMessageId: string) => {
    if (!userMessageId) {
      return;
    }
    void toggleTurnProcess(userMessageId).catch((error) => {
      console.error('Failed to toggle turn process messages in team pane:', error);
    });
  }, [toggleTurnProcess]);

  const handleSendMessage = useCallback(async (
    content: string,
    attachments?: File[],
    runtimeOptions?: {
      mcpEnabled?: boolean;
      projectId?: string | null;
      projectRoot?: string | null;
      workspaceRoot?: string | null;
      enabledMcpIds?: string[];
    },
  ) => {
    if (!selectedContact) {
      return;
    }
    try {
      const sessionId = await ensureContactSession(selectedContact);
      if (!sessionId) {
        return;
      }
      await sendMessage(content, attachments, {
        mcpEnabled: runtimeOptions?.mcpEnabled,
        enabledMcpIds: runtimeOptions?.enabledMcpIds,
        contactAgentId: selectedContact.agentId,
        contactId: selectedContact.id,
        projectId: normalizedProjectId,
        projectRoot: project.rootPath || null,
        workspaceRoot: null,
      });
    } catch (error) {
      console.error('Failed to send message in team pane:', error);
    }
  }, [
    ensureContactSession,
    normalizedProjectId,
    project.rootPath,
    selectedContact,
    sendMessage,
  ]);

  const persistTeamSessionMcpRuntime = useCallback((nextEnabled: boolean, nextIdsInput: string[]) => {
    const targetSession = selectedProjectSession || currentSession;
    if (!targetSession?.id) {
      return;
    }

    const normalizedIds: string[] = [];
    for (const item of Array.isArray(nextIdsInput) ? nextIdsInput : []) {
      const trimmed = typeof item === 'string' ? item.trim() : '';
      if (!trimmed || normalizedIds.includes(trimmed)) {
        continue;
      }
      normalizedIds.push(trimmed);
    }

    const runtime = readSessionRuntimeFromMetadata(targetSession.metadata);
    const currentEnabled = runtime?.mcpEnabled ?? true;
    const currentIds = Array.isArray(runtime?.enabledMcpIds) ? runtime.enabledMcpIds : [];
    const sameIds = currentIds.length === normalizedIds.length
      && currentIds.every((id, index) => id === normalizedIds[index]);
    if (currentEnabled === nextEnabled && sameIds) {
      return;
    }

    const metadata = mergeSessionRuntimeIntoMetadata(targetSession.metadata, {
      mcpEnabled: nextEnabled,
      enabledMcpIds: normalizedIds,
    });
    void updateSession(targetSession.id, { metadata } as any);
  }, [currentSession, selectedProjectSession, updateSession]);

  const handleComposerMcpEnabledChange = useCallback((enabled: boolean) => {
    setComposerMcpEnabled(enabled);
    persistTeamSessionMcpRuntime(enabled, composerEnabledMcpIds);
  }, [composerEnabledMcpIds, persistTeamSessionMcpRuntime]);

  const handleComposerEnabledMcpIdsChange = useCallback((ids: string[]) => {
    const normalizedIds: string[] = [];
    for (const item of Array.isArray(ids) ? ids : []) {
      const trimmed = typeof item === 'string' ? item.trim() : '';
      if (!trimmed || normalizedIds.includes(trimmed)) {
        continue;
      }
      normalizedIds.push(trimmed);
    }
    setComposerEnabledMcpIds(normalizedIds);
    persistTeamSessionMcpRuntime(composerMcpEnabled, normalizedIds);
  }, [composerMcpEnabled, persistTeamSessionMcpRuntime]);

  const loadSessionSummaries = useCallback(async (
    sessionId: string,
    options?: { silent?: boolean },
  ) => {
    if (!sessionId) {
      setSummaryItems([]);
      setSummaryError(null);
      setSummaryLoading(false);
      return;
    }
    if (!options?.silent) {
      setSummaryLoading(true);
    }
    setSummaryError(null);
    try {
      const result = await apiClient.getSessionSummaries(sessionId, { limit: 200, offset: 0 });
      const normalized = (Array.isArray(result?.items) ? result.items : [])
        .map((item: any) => normalizeSessionSummary(item))
        .filter((item: SessionSummaryItem | null): item is SessionSummaryItem => Boolean(item))
        .sort((left, right) => {
          const leftTs = new Date(left.createdAt || left.updatedAt).getTime();
          const rightTs = new Date(right.createdAt || right.updatedAt).getTime();
          return (Number.isFinite(rightTs) ? rightTs : 0) - (Number.isFinite(leftTs) ? leftTs : 0);
        });
      setSummaryItems(normalized);
    } catch (error) {
      setSummaryError(error instanceof Error ? error.message : '加载会话总结失败');
      setSummaryItems([]);
    } finally {
      setSummaryLoading(false);
    }
  }, [apiClient]);

  const handleOpenSummary = useCallback(async (contact: ContactItem) => {
    setOpeningSummaryContactId(contact.id);
    setSelectedContactId(contact.id);
    setSwitchingContactId(contact.id);
    setSummaryError(null);
    try {
      const sessionId = await ensureContactSession(contact);
      if (!sessionId) {
        return;
      }
      if (summaryPaneSessionId === sessionId) {
        setSummaryPaneSessionId(null);
        return;
      }
      setSummaryPaneSessionId(sessionId);
      await loadSessionSummaries(sessionId);
    } finally {
      setSwitchingContactId((prev) => (prev === contact.id ? null : prev));
      setOpeningSummaryContactId((prev) => (prev === contact.id ? null : prev));
    }
  }, [ensureContactSession, loadSessionSummaries, summaryPaneSessionId]);

  const handleDeleteSummary = useCallback(async (summaryId: string) => {
    if (!selectedProjectSession?.id || !summaryId) {
      return;
    }
    setDeletingSummaryId(summaryId);
    setSummaryError(null);
    try {
      await apiClient.deleteSessionSummary(selectedProjectSession.id, summaryId);
      await loadSessionSummaries(selectedProjectSession.id, { silent: true });
    } catch (error) {
      setSummaryError(error instanceof Error ? error.message : '删除总结失败');
    } finally {
      setDeletingSummaryId((prev) => (prev === summaryId ? null : prev));
    }
  }, [apiClient, loadSessionSummaries, selectedProjectSession?.id]);

  const handleClearSummaries = useCallback(async () => {
    if (!selectedProjectSession?.id) {
      return;
    }
    const confirmed = typeof window === 'undefined'
      ? true
      : window.confirm('确定清空当前会话的所有总结吗？');
    if (!confirmed) {
      return;
    }
    setClearingSummaries(true);
    setSummaryError(null);
    try {
      await apiClient.clearSessionSummaries(selectedProjectSession.id);
      await loadSessionSummaries(selectedProjectSession.id, { silent: true });
    } catch (error) {
      setSummaryError(error instanceof Error ? error.message : '清空总结失败');
    } finally {
      setClearingSummaries(false);
    }
  }, [apiClient, loadSessionSummaries, selectedProjectSession?.id]);

  const handleOpenAddMember = useCallback(async () => {
    setMemberPickerError(null);
    let latestContacts = contacts || [];
    try {
      const loaded = await loadContacts();
      if (Array.isArray(loaded) && loaded.length > 0) {
        latestContacts = loaded as any[];
      }
    } catch (error) {
      setMemberPickerError(error instanceof Error ? error.message : '加载联系人失败');
    }
    const firstAvailable = latestContacts.find((item: any) => !projectContactIdSet.has(item.id));
    setMemberPickerSelectedId(firstAvailable?.id || null);
    setMemberPickerOpen(true);
  }, [contacts, loadContacts, projectContactIdSet]);

  const handleConfirmAddMember = useCallback(async () => {
    const contactId = memberPickerSelectedId?.trim() || '';
    if (!contactId) {
      setMemberPickerError('请先选择联系人');
      return;
    }
    try {
      await apiClient.addProjectContact(project.id, { contact_id: contactId });
      emitProjectContactChanged(project.id);
      setMemberPickerOpen(false);
      setMemberPickerSelectedId(null);
      setMemberPickerError(null);
      const selected = (contacts || []).find((item: ContactItem) => item.id === contactId) || null;
      if (selected) {
        await handleSelectContact(selected.id);
      }
    } catch (error) {
      setMemberPickerError(error instanceof Error ? error.message : '添加项目成员失败');
    }
  }, [
    apiClient,
    contacts,
    emitProjectContactChanged,
    handleSelectContact,
    memberPickerSelectedId,
    project.id,
  ]);

  const handleRemoveMember = useCallback(async (contact: ContactItem) => {
    const confirmed = typeof window === 'undefined'
      ? true
      : window.confirm(`确定将 ${contact.name} 从当前项目团队中移除吗？`);
    if (!confirmed) {
      return;
    }
    setMemberPickerError(null);
    setRemovingContactId(contact.id);
    try {
      await apiClient.removeProjectContact(project.id, contact.id);
      emitProjectContactChanged(project.id);
      if (selectedContactId === contact.id) {
        setSelectedContactId(null);
        setSummaryPaneSessionId(null);
        setSummaryItems([]);
        setSummaryError(null);
      }
    } catch (error) {
      setMemberPickerError(error instanceof Error ? error.message : '移除项目成员失败');
    } finally {
      setRemovingContactId((prev) => (prev === contact.id ? null : prev));
    }
  }, [apiClient, emitProjectContactChanged, project.id, selectedContactId]);

  useEffect(() => {
    if (!sessionSummaryPaneVisible || !selectedProjectSession?.id) {
      return;
    }
    void loadSessionSummaries(selectedProjectSession.id, { silent: true });
  }, [loadSessionSummaries, selectedProjectSession?.id, sessionSummaryPaneVisible]);

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        请选择一个项目
      </div>
    );
  }

  return (
    <div className={cn('flex h-full overflow-hidden', className)}>
      <div className="w-64 shrink-0 border-r border-border bg-card/40 flex flex-col">
        <div className="px-3 py-2 border-b border-border space-y-2">
          <div className="flex items-center justify-between gap-2">
            <div className="text-xs uppercase tracking-wide text-muted-foreground">团队成员</div>
            <button
              type="button"
              className="px-2 py-1 text-xs rounded border border-border text-muted-foreground hover:text-foreground hover:bg-accent"
              onClick={() => { void handleOpenAddMember(); }}
            >
              添加
            </button>
          </div>
          <div className="text-sm font-medium text-foreground truncate" title={project.name}>{project.name}</div>
          {projectMembersError && (
            <div className="text-[11px] text-destructive">{projectMembersError}</div>
          )}
          {memberPickerError && (
            <div className="text-[11px] text-destructive">{memberPickerError}</div>
          )}
        </div>
        <div className="flex-1 min-h-0 overflow-y-auto p-2 space-y-1">
          {projectMembersLoading ? (
            <div className="text-xs text-muted-foreground px-2 py-3">
              正在加载项目成员...
            </div>
          ) : projectContacts.length === 0 ? (
            <div className="text-xs text-muted-foreground px-2 py-3">
              当前项目暂无已添加联系人，请点击上方“添加”按钮。
            </div>
          ) : (
            projectContacts.map(({ contact, session }) => {
              const active = selectedContactId === contact.id;
              const switching = switchingContactId === contact.id;
              const chatState = session?.id ? sessionChatState?.[session.id] : undefined;
              const isBusy = Boolean(chatState?.isLoading || chatState?.isStreaming);
              return (
                <button
                  key={contact.id}
                  type="button"
                  onClick={() => { void handleSelectContact(contact.id); }}
                  className={cn(
                    'w-full text-left rounded-md border px-2 py-2 transition-colors',
                    active
                      ? 'bg-accent border-border'
                      : 'border-transparent hover:bg-accent/60'
                  )}
                >
                  <div className="flex items-center justify-between gap-2">
                    <div className="text-sm font-medium text-foreground truncate">{contact.name}</div>
                    <div className="flex items-center gap-1 shrink-0">
                      <button
                        type="button"
                        className={cn(
                          'px-1.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground hover:text-foreground hover:bg-accent',
                          summaryPaneSessionId && session?.id === summaryPaneSessionId && 'text-blue-600 border-blue-200',
                        )}
                        onClick={(event) => {
                          event.stopPropagation();
                          void handleOpenSummary(contact);
                        }}
                        disabled={openingSummaryContactId === contact.id}
                      >
                        {openingSummaryContactId === contact.id
                          ? '加载中'
                          : (summaryPaneSessionId && session?.id === summaryPaneSessionId ? '关闭总结' : '总结')}
                      </button>
                      <button
                        type="button"
                        className="px-1.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground hover:text-destructive hover:border-destructive"
                        onClick={(event) => {
                          event.stopPropagation();
                          void handleRemoveMember(contact);
                        }}
                        disabled={removingContactId === contact.id}
                      >
                        {removingContactId === contact.id ? '移除中' : '移除'}
                      </button>
                    </div>
                  </div>
                  <div className="mt-1 text-[11px] text-muted-foreground truncate">
                    {switching ? (
                      '切换中...'
                    ) : (
                      <span className="inline-flex items-center gap-2">
                        <span>{`会话: ${session?.title || '未创建'}`}</span>
                        {session?.id ? (
                          <span
                            className={cn(
                              'inline-flex items-center gap-1',
                              isBusy ? 'text-amber-600' : 'text-muted-foreground',
                            )}
                          >
                            <span
                              className={cn(
                                'inline-block w-2 h-2 rounded-full',
                                isBusy ? 'bg-amber-500' : 'bg-muted-foreground/40',
                              )}
                            />
                            {isBusy ? '执行中' : '空闲'}
                          </span>
                        ) : null}
                      </span>
                    )}
                  </div>
                </button>
              );
            })
          )}
        </div>
      </div>

      <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
        <div className="flex-1 overflow-hidden">
          {!selectedContact ? (
            <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
              请选择一个团队成员开始对话
            </div>
          ) : !selectedProjectSession ? (
            <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
              正在准备会话...
            </div>
          ) : !isSelectedSessionActive ? (
            <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
              正在切换到 {selectedContact.name} 的会话...
            </div>
          ) : sessionSummaryPaneVisible ? (
            <div className="h-full min-h-0 flex flex-col overflow-hidden">
              <div className="basis-[42%] min-h-[170px] bg-card/40 flex flex-col overflow-hidden border-b border-border">
                <div className="px-3 py-2 border-b border-border flex items-center justify-between gap-2">
                  <div className="min-w-0">
                    <div className="text-sm font-medium truncate">会话总结</div>
                    <div className="text-[11px] text-muted-foreground truncate">
                      {selectedContact.name || selectedProjectSession.title || '当前联系人'}
                    </div>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    <button
                      type="button"
                      className="px-2 py-1 text-xs rounded border border-border hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
                      disabled={clearingSummaries || summaryLoading}
                      onClick={() => { void handleClearSummaries(); }}
                    >
                      {clearingSummaries ? '清空中...' : '清空所有总结'}
                    </button>
                    <button
                      type="button"
                      className="px-2 py-1 text-xs rounded border border-border hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
                      disabled={summaryLoading}
                      onClick={() => { void loadSessionSummaries(selectedProjectSession.id); }}
                    >
                      {summaryLoading ? '刷新中...' : '刷新'}
                    </button>
                    <button
                      type="button"
                      className="px-2 py-1 text-xs rounded border border-border hover:bg-accent"
                      onClick={() => setSummaryPaneSessionId(null)}
                    >
                      关闭
                    </button>
                  </div>
                </div>
                <div className="flex-1 min-h-0 overflow-y-auto px-3 py-2 space-y-2">
                  {summaryError ? (
                    <div className="text-xs text-destructive">{summaryError}</div>
                  ) : null}
                  {summaryLoading ? (
                    <div className="text-xs text-muted-foreground">正在加载会话总结...</div>
                  ) : summaryItems.length === 0 ? (
                    <div className="text-xs text-muted-foreground">当前会话暂无总结。</div>
                  ) : (
                    summaryItems.map((item) => (
                      <div key={item.id} className="rounded-md border border-border bg-background/80 p-2">
                        <div className="flex items-center justify-between gap-2">
                          <div className="min-w-0 text-[12px] text-muted-foreground truncate">
                            {item.triggerType || 'summary'}
                            {item.level > 0 ? ` · level ${item.level}` : ''}
                          </div>
                          <div className="flex items-center gap-2 shrink-0">
                            <div className="text-[11px] text-muted-foreground">
                              {formatSummaryTime(item.createdAt)}
                            </div>
                            <button
                              type="button"
                              className="px-1.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground hover:text-destructive hover:border-destructive disabled:opacity-60"
                              onClick={() => { void handleDeleteSummary(item.id); }}
                              disabled={deletingSummaryId === item.id}
                            >
                              {deletingSummaryId === item.id ? '删除中' : '删除'}
                            </button>
                          </div>
                        </div>
                        <div className="mt-1 text-[11px] text-muted-foreground">
                          {`消息 ${item.sourceMessageCount} · 估算 ${item.sourceEstimatedTokens} tok`}
                        </div>
                        {item.status && item.status !== 'summarized' && (
                          <div className="mt-1 text-[11px] text-amber-600">
                            {item.status}
                          </div>
                        )}
                        {item.errorMessage && (
                          <div className="mt-1 text-[11px] text-destructive">
                            {item.errorMessage}
                          </div>
                        )}
                        <div className="mt-2 text-sm leading-6 whitespace-pre-wrap break-words">
                          {item.summaryText || '(空总结)'}
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </div>
              <div className="relative shrink-0 px-3 py-1.5 bg-card/20">
                <div className="h-[2px] rounded-full bg-gradient-to-r from-transparent via-sky-400/95 to-transparent shadow-[0_0_16px_rgba(56,189,248,0.95)]" />
                <div className="pointer-events-none absolute inset-x-0 top-0 h-full bg-gradient-to-b from-sky-400/10 via-transparent to-transparent" />
              </div>
              <div className="flex-1 min-h-0 overflow-hidden">
                <MessageList
                  key={`project-team-messages-${selectedProjectSession.id}-summary`}
                  sessionId={selectedProjectSession.id}
                  messages={messages}
                  isLoading={chatIsLoading}
                  isStreaming={chatIsStreaming}
                  isStopping={chatIsStopping}
                  hasMore={hasMoreMessages}
                  onLoadMore={handleLoadMore}
                  onToggleTurnProcess={handleToggleTurnProcess}
                />
              </div>
            </div>
          ) : (
            <MessageList
              key={`project-team-messages-${selectedProjectSession.id}`}
              sessionId={selectedProjectSession.id}
              messages={messages}
              isLoading={chatIsLoading}
              isStreaming={chatIsStreaming}
              isStopping={chatIsStopping}
              hasMore={hasMoreMessages}
              onLoadMore={handleLoadMore}
              onToggleTurnProcess={handleToggleTurnProcess}
            />
          )}
        </div>

        {selectedContact && (
          <InputArea
            onSend={handleSendMessage}
            onStop={abortCurrentConversation}
            disabled={!isSelectedSessionActive || chatIsLoading || chatIsStreaming || chatIsStopping}
            isStreaming={chatIsStreaming}
            isStopping={chatIsStopping}
            placeholder={`给 ${selectedContact.name} 发送消息...`}
            allowAttachments={true}
            showModelSelector={true}
            selectedModelId={selectedModelId}
            availableModels={aiModelConfigs}
            onModelChange={setSelectedModel}
            reasoningSupported={supportsReasoning}
            reasoningEnabled={chatConfig?.reasoningEnabled === true}
            onReasoningToggle={(enabled) => updateChatConfig({ reasoningEnabled: enabled })}
            availableProjects={[project]}
            currentProject={project}
            selectedProjectId={project.id}
            onProjectChange={() => {}}
            showProjectSelector={false}
            showProjectFileButton={false}
            showWorkspaceRootPicker={false}
            mcpEnabled={composerMcpEnabled}
            enabledMcpIds={composerEnabledMcpIds}
            onMcpEnabledChange={handleComposerMcpEnabledChange}
            onEnabledMcpIdsChange={handleComposerEnabledMcpIdsChange}
          />
        )}
      </div>
      <ProjectContactPickerModal
        isOpen={memberPickerOpen}
        projectName={project.name}
        contacts={projectContactsOptions}
        disabledContactIds={Array.from(projectContactIdSet)}
        selectedContactId={memberPickerSelectedId}
        error={memberPickerError}
        onClose={() => {
          setMemberPickerOpen(false);
          setMemberPickerSelectedId(null);
          setMemberPickerError(null);
        }}
        onSelectedContactChange={(contactId) => {
          setMemberPickerSelectedId(contactId);
          setMemberPickerError(null);
        }}
        onConfirm={() => {
          void handleConfirmAddMember();
        }}
      />
    </div>
  );
};

export default TeamMembersPane;
