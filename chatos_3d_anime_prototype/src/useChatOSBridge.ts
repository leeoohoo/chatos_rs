import { useCallback, useRef, useState } from 'react';
import { prepareAttachmentPayloads } from './chatAttachments';
import { useBridgeDerivedData } from './chatOSBridge/useDerivedData';
import { useBridgeRealtime } from './chatOSBridge/useRealtime';
import {
  API_BASE_URL,
  EMPTY_TASK_GRAPH,
  apiRequest,
  findContactSession,
  formatMessageTime,
  formatRelativeTime,
  latestSession,
  mapAgent,
  mapMessages,
  mapModel,
  mapProject,
  mapTask,
  mapTaskRunnerGraph,
  normalizeRuntimeSettings,
  persistAuth,
  rawTaskTime,
  readStoredAuth,
  sessionIdentity,
  value,
  type BridgeStatus,
  type RawAgent,
  type RawContact,
  type RawMessage,
  type RawModelConfig,
  type RawProject,
  type RawProjectContact,
  type RawProjectPlan,
  type RawRuntimeSettings,
  type RawSession,
  type RawTask,
  type RawTaskRunnerGraph,
  type RealtimeEnvelope,
  type StoredAuth,
  type WebSocketStatus,
  type WorkspaceSnapshot,
} from './chatOSBridge/support';
import type {
  ChatContact,
  ChatMessage,
  ChatRuntimeSettings,
  DemoProject,
  DemoTask,
  DemoTaskGraph,
} from './types';

export function useChatOSBridge() {
  const [auth, setAuth] = useState<StoredAuth | null>(() => readStoredAuth());
  const [status, setStatus] = useState<BridgeStatus>(auth ? 'connecting' : 'demo');
  const [webSocketStatus, setWebSocketStatus] = useState<WebSocketStatus>('idle');
  const [pageVisible, setPageVisible] = useState(() => typeof document === 'undefined' || document.visibilityState !== 'hidden');
  const [error, setError] = useState<string | null>(null);
  const [rawProjects, setRawProjects] = useState<RawProject[]>([]);
  const [rawSessions, setRawSessions] = useState<RawSession[]>([]);
  const [modelConfigs, setModelConfigs] = useState<RawModelConfig[]>([]);
  const [rawContacts, setRawContacts] = useState<RawContact[]>([]);
  const [rawAgents, setRawAgents] = useState<RawAgent[]>([]);
  const [rawProjectContacts, setRawProjectContacts] = useState<RawProjectContact[]>([]);
  const [projects, setProjects] = useState<DemoProject[]>([]);
  const [tasks, setTasks] = useState<DemoTask[]>([]);
  const [runningTasks, setRunningTasks] = useState<DemoTask[]>([]);
  const [taskGraph, setTaskGraph] = useState<DemoTaskGraph>(EMPTY_TASK_GRAPH);
  const [taskGraphTaskId, setTaskGraphTaskId] = useState<string | null>(null);
  const [taskGraphLoading, setTaskGraphLoading] = useState(false);
  const [taskGraphError, setTaskGraphError] = useState<string | null>(null);
  const [persistedMessages, setPersistedMessages] = useState<ChatMessage[]>([]);
  const [streamingText, setStreamingText] = useState('');
  const [thinking, setThinking] = useState(false);
  const [isStopping, setIsStopping] = useState(false);
  const [loadingMessages, setLoadingMessages] = useState(false);
  const [hasMoreMessages, setHasMoreMessages] = useState(false);
  const [messageLimit, setMessageLimit] = useState(40);
  const [sessionBusy, setSessionBusy] = useState(false);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [conversationTitle, setConversationTitle] = useState<string | null>(null);
  const [activeModel, setActiveModel] = useState<RawModelConfig | null>(null);
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [activeContactId, setActiveContactId] = useState<string | null>(null);
  const [runtimeSettings, setRuntimeSettings] = useState<ChatRuntimeSettings>({
    selectedModelId: null,
    selectedModelName: null,
    selectedThinkingLevel: null,
    reasoningEnabled: true,
    planModeEnabled: false,
  });
  const refreshTimerRef = useRef<number | null>(null);
  const activeTurnIdRef = useRef<string | null>(null);


  const { messages, models, agents, accountContacts, contacts, availableAgents } = useBridgeDerivedData({
    persistedMessages,
    streamingText,
    modelConfigs,
    rawAgents,
    rawContacts,
    rawSessions,
    rawProjectContacts,
    activeProjectId,
  });


  const loadConversationData = useCallback(async (
    token: string,
    session: RawSession,
    configs: RawModelConfig[],
    limit = 40,
  ) => {
    setLoadingMessages(true);
    try {
      const [messageItems, taskResult, rawRuntime] = await Promise.all([
        apiRequest<RawMessage[]>(`/conversations/${encodeURIComponent(session.id)}/messages?limit=${limit}`, token),
        apiRequest<{ tasks?: RawTask[] } | RawTask[]>(`/task-manager/tasks?conversation_id=${encodeURIComponent(session.id)}&include_done=true&limit=30`, token),
        apiRequest<RawRuntimeSettings>(`/conversations/${encodeURIComponent(session.id)}/runtime-settings`, token)
          .catch((): RawRuntimeSettings => ({})),
      ]);
      const rawTasks = Array.isArray(taskResult) ? taskResult : Array.isArray(taskResult.tasks) ? taskResult.tasks : [];
      const selectedModelId = String(rawRuntime.selected_model_id || session.selected_model_id || '').trim();
      const model = configs.find((item) => item.id === selectedModelId)
        || configs.find((item) => item.enabled !== false)
        || configs[0]
        || null;
      setConversationId(session.id);
      setConversationTitle(session.title);
      const identity = sessionIdentity(session);
      setActiveProjectId(identity.projectId);
      setActiveContactId(identity.contactId);
      setActiveModel(model);
      setRuntimeSettings(normalizeRuntimeSettings(rawRuntime, session, model));
      setPersistedMessages(mapMessages(messageItems));
      setMessageLimit(limit);
      setHasMoreMessages(messageItems.length >= limit);
      setTasks(rawTasks.map(mapTask));
      setStreamingText('');
      setThinking(false);
      setIsStopping(false);
    } finally {
      setLoadingMessages(false);
    }
  }, []);

  const loadSnapshot = useCallback(async (token: string): Promise<WorkspaceSnapshot> => {
    const [projectItems, sessionItems, taskSessionItems, configs, contacts, agents] = await Promise.all([
      apiRequest<RawProject[]>('/projects', token),
      apiRequest<RawSession[]>('/conversations?limit=80&include_archived=false', token),
      apiRequest<RawSession[]>('/conversations?limit=160&include_archived=true', token)
        .catch(() => [] as RawSession[]),
      apiRequest<RawModelConfig[]>('/ai-model-configs', token),
      apiRequest<RawContact[]>('/contacts?limit=200', token),
      apiRequest<RawAgent[]>('/agents?enabled=true&limit=200', token),
    ]);
    return {
      projects: projectItems,
      sessions: sessionItems,
      taskSessions: taskSessionItems.length > 0 ? taskSessionItems : sessionItems,
      modelConfigs: configs,
      contacts,
      agents,
    };
  }, []);

  const loadWorkspaceTasks = useCallback(async (token: string, sessions: RawSession[]) => {
    if (sessions.length === 0) {
      setRunningTasks([]);
      return [] as DemoTask[];
    }

    const rows: Array<{ task: RawTask; session: RawSession }> = [];
    let nextIndex = 0;
    const workerCount = Math.min(6, sessions.length);
    const workers = Array.from({ length: workerCount }, async () => {
      while (nextIndex < sessions.length) {
        const session = sessions[nextIndex];
        nextIndex += 1;
        try {
          const result = await apiRequest<{ tasks?: RawTask[] } | RawTask[]>(
            `/task-manager/tasks?conversation_id=${encodeURIComponent(session.id)}&include_done=true&limit=100`,
            token,
          );
          const sessionTasks = Array.isArray(result) ? result : Array.isArray(result.tasks) ? result.tasks : [];
          sessionTasks.forEach((task) => rows.push({ task, session }));
        } catch {
          // A deleted or temporarily unavailable conversation should not hide history from other sessions.
        }
      }
    });

    await Promise.all(workers);
    const mapped = rows
      .sort((left, right) => rawTaskTime(right.task) - rawTaskTime(left.task))
      .map(({ task, session }) => ({
        ...mapTask(task),
        id: `${session.id}:${task.id}`,
        conversationId: session.id,
        conversationTitle: session.title || '未命名会话',
        updatedAt: formatRelativeTime(task.updated_at || task.created_at),
      }));
    setRunningTasks(mapped);
    return mapped;
  }, []);

  const loadProjectContactRows = useCallback(async (token: string, projectId: string | null) => {
    if (!projectId) {
      setRawProjectContacts([]);
      return [] as RawProjectContact[];
    }
    const rows = await apiRequest<RawProjectContact[]>(`/projects/${encodeURIComponent(projectId)}/contacts?limit=200`, token);
    setRawProjectContacts(rows);
    return rows;
  }, []);

  const refreshSessionList = useCallback(async (token: string) => {
    const items = await apiRequest<RawSession[]>('/conversations?limit=80&include_archived=false', token);
    setRawSessions(items);
    void apiRequest<RawSession[]>('/conversations?limit=160&include_archived=true', token)
      .then((taskSessions) => loadWorkspaceTasks(token, taskSessions.length > 0 ? taskSessions : items))
      .catch(() => loadWorkspaceTasks(token, items));
    return items;
  }, [loadWorkspaceTasks]);

  const refreshProjectList = useCallback(async (token: string) => {
    const items = await apiRequest<RawProject[]>('/projects', token);
    setRawProjects(items);
    setProjects(items.map((project, index) => mapProject(project, index)));
  }, []);

  const refreshContactList = useCallback(async (token: string) => {
    const items = await apiRequest<RawContact[]>('/contacts?limit=200', token);
    setRawContacts(items);
  }, []);

  const refresh = useCallback(async () => {
    if (!auth?.accessToken) {
      setStatus('demo');
      return;
    }
    const preserveLiveWorkspace = status === 'live';
    if (!preserveLiveWorkspace) setStatus('connecting');
    setError(null);
    try {
      const snapshot = await loadSnapshot(auth.accessToken);
      setRawProjects(snapshot.projects);
      setRawSessions(snapshot.sessions);
      setModelConfigs(snapshot.modelConfigs);
      setRawContacts(snapshot.contacts);
      setRawAgents(snapshot.agents);
      setProjects(snapshot.projects.map((project, index) => mapProject(project, index)));
      const workspaceTasksPromise = loadWorkspaceTasks(auth.accessToken, snapshot.taskSessions);
      const preferred = snapshot.sessions.find((session) => session.id === conversationId)
        || latestSession(snapshot.sessions.filter((session) => {
          const identity = sessionIdentity(session);
          return identity.projectId === null && Boolean(identity.contactId || identity.agentId);
      }));
      if (preferred) {
        const projectRows = await loadProjectContactRows(auth.accessToken, sessionIdentity(preferred).projectId);
        await loadConversationData(auth.accessToken, preferred, snapshot.modelConfigs);
        if (!sessionIdentity(preferred).contactId) {
          const matchedRow = projectRows.find((item) => value(item.latest_session_id, item.latestSessionId) === preferred.id);
          if (matchedRow) setActiveContactId(value(matchedRow.contact_id, matchedRow.contactId) || null);
        }
      } else {
        const firstContact = snapshot.contacts[0];
        const firstAgentId = value(firstContact?.agent_id, firstContact?.agentId);
        const firstAgent = snapshot.agents.find((item) => item.id === firstAgentId);
        setConversationId(null);
        setConversationTitle(firstContact ? value(firstContact.agent_name_snapshot, firstContact.agentNameSnapshot) || firstAgent?.name || '联系人' : null);
        setTasks([]);
        setPersistedMessages([]);
        setActiveProjectId(null);
        setActiveContactId(firstContact?.id || null);
        setRawProjectContacts([]);
        setHasMoreMessages(false);
        setRuntimeSettings({
          selectedModelId: null,
          selectedModelName: null,
          selectedThinkingLevel: null,
          reasoningEnabled: true,
          planModeEnabled: false,
        });
      }
      await workspaceTasksPromise;
      setStatus('live');
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setError(message);
      setStatus(preserveLiveWorkspace ? 'live' : 'error');
    }
  }, [auth, conversationId, loadConversationData, loadProjectContactRows, loadSnapshot, loadWorkspaceTasks, status]);

  const refreshConversation = useCallback(async () => {
    if (!auth?.accessToken || !conversationId) return;
    const session = rawSessions.find((item) => item.id === conversationId);
    if (!session) return;
    await loadConversationData(auth.accessToken, session, modelConfigs, messageLimit);
  }, [auth, conversationId, loadConversationData, messageLimit, modelConfigs, rawSessions]);

  const scheduleConversationRefresh = useCallback(() => {
    if (refreshTimerRef.current !== null) window.clearTimeout(refreshTimerRef.current);
    refreshTimerRef.current = window.setTimeout(() => {
      refreshTimerRef.current = null;
      void refreshConversation();
    }, 240);
  }, [refreshConversation]);


  useBridgeRealtime({
    activeProjectId, auth, conversationId, pageVisible, rawSessions, status,
    activeTurnIdRef, refreshTimerRef, loadProjectContactRows, loadWorkspaceTasks,
    refresh, refreshContactList, refreshProjectList, refreshSessionList,
    scheduleConversationRefresh, setError, setIsStopping, setPageVisible,
    setRunningTasks, setStreamingText, setThinking, setWebSocketStatus,
  });


  const loadTaskGraph = useCallback(async (task: DemoTask) => {
    setTaskGraphTaskId(task.id);
    setTaskGraphError(null);
    if (!auth?.accessToken || !task.conversationId || (!task.conversationTurnId && !task.sourceUserMessageId)) {
      setTaskGraph(EMPTY_TASK_GRAPH);
      setTaskGraphLoading(false);
      if (auth?.accessToken) setTaskGraphError('这个任务没有关联的会话轮次，暂时无法读取依赖图。');
      return EMPTY_TASK_GRAPH;
    }

    setTaskGraphLoading(true);
    setTaskGraph(EMPTY_TASK_GRAPH);
    try {
      const query = new URLSearchParams();
      query.set('session_id', task.conversationId);
      if (task.conversationTurnId) query.set('turn_id', task.conversationTurnId);
      if (task.sourceUserMessageId) query.set('source_user_message_id', task.sourceUserMessageId);
      const messageId = task.sourceUserMessageId || `task-source-${task.id}`;
      const response = await apiRequest<RawTaskRunnerGraph>(
        `/messages/${encodeURIComponent(messageId)}/task-runner/graph?${query.toString()}`,
        auth.accessToken,
      );
      const mapped = mapTaskRunnerGraph(response);
      setTaskGraph(mapped);
      if (mapped.nodes.length === 0) {
        setTaskGraphError('该任务轮次没有 Task Runner 依赖图记录。');
      }
      return mapped;
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setTaskGraph(EMPTY_TASK_GRAPH);
      setTaskGraphError(message || '读取任务流程图失败');
      return EMPTY_TASK_GRAPH;
    } finally {
      setTaskGraphLoading(false);
    }
  }, [auth]);

  const login = useCallback(async (username: string, password: string) => {
    setStatus('connecting');
    setError(null);
    try {
      const response = await apiRequest<{
        access_token?: string;
        token?: string;
        user?: { id?: string; username?: string } | null;
      }>('/auth/login', null, {
        method: 'POST',
        body: JSON.stringify({ username, password }),
      });
      const accessToken = String(response.access_token || response.token || '').trim();
      const userId = String(response.user?.id || response.user?.username || username).trim();
      if (!accessToken || !userId) throw new Error('登录响应缺少令牌或用户信息');
      const nextAuth: StoredAuth = {
        accessToken,
        user: { id: userId, username: String(response.user?.username || username) },
      };
      persistAuth(nextAuth);
      setAuth(nextAuth);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setStatus('error');
      throw cause;
    }
  }, []);

  const logout = useCallback(() => {
    persistAuth(null);
    setAuth(null);
    setStatus('demo');
    setError(null);
    setRawProjects([]);
    setRawSessions([]);
    setModelConfigs([]);
    setRawContacts([]);
    setRawAgents([]);
    setRawProjectContacts([]);
    setProjects([]);
    setTasks([]);
    setRunningTasks([]);
    setTaskGraph(EMPTY_TASK_GRAPH);
    setTaskGraphTaskId(null);
    setTaskGraphLoading(false);
    setTaskGraphError(null);
    setPersistedMessages([]);
    setConversationId(null);
    setConversationTitle(null);
    setActiveModel(null);
    setActiveProjectId(null);
    setActiveContactId(null);
    setStreamingText('');
    setThinking(false);
    setIsStopping(false);
    setLoadingMessages(false);
    setHasMoreMessages(false);
    setMessageLimit(40);
    setSessionBusy(false);
    setRuntimeSettings({
      selectedModelId: null,
      selectedModelName: null,
      selectedThinkingLevel: null,
      reasoningEnabled: true,
      planModeEnabled: false,
    });
    activeTurnIdRef.current = null;
  }, []);

  const loadMoreMessages = useCallback(async () => {
    if (!auth?.accessToken || !conversationId || loadingMessages || !hasMoreMessages) return;
    const session = rawSessions.find((item) => item.id === conversationId);
    if (!session) return;
    await loadConversationData(auth.accessToken, session, modelConfigs, messageLimit + 40);
  }, [auth, conversationId, hasMoreMessages, loadConversationData, loadingMessages, messageLimit, modelConfigs, rawSessions]);

  const updateRuntimeSettings = useCallback(async (patch: Partial<ChatRuntimeSettings>) => {
    if (!auth?.accessToken) return;
    const body: RawRuntimeSettings = {};
    if (Object.prototype.hasOwnProperty.call(patch, 'selectedModelId')) body.selected_model_id = patch.selectedModelId ?? null;
    if (Object.prototype.hasOwnProperty.call(patch, 'selectedModelName')) body.selected_model_name = patch.selectedModelName ?? null;
    if (Object.prototype.hasOwnProperty.call(patch, 'selectedThinkingLevel')) body.selected_thinking_level = patch.selectedThinkingLevel ?? null;
    if (Object.prototype.hasOwnProperty.call(patch, 'reasoningEnabled')) body.reasoning_enabled = patch.reasoningEnabled === true;
    if (Object.prototype.hasOwnProperty.call(patch, 'planModeEnabled')) body.plan_mode_enabled = patch.planModeEnabled === true;
    const previous = runtimeSettings;
    const optimistic = { ...runtimeSettings, ...patch };
    setRuntimeSettings(optimistic);
    if (patch.selectedModelId) {
      const model = modelConfigs.find((item) => item.id === patch.selectedModelId) || null;
      if (model) setActiveModel(model);
    }
    if (!conversationId) return;
    try {
      const saved = await apiRequest<RawRuntimeSettings>(`/conversations/${encodeURIComponent(conversationId)}/runtime-settings`, auth.accessToken, {
        method: 'PUT',
        body: JSON.stringify(body),
      });
      const session = rawSessions.find((item) => item.id === conversationId) || { id: conversationId, title: conversationTitle || '当前会话' };
      const selected = modelConfigs.find((item) => item.id === saved.selected_model_id)
        || modelConfigs.find((item) => item.id === optimistic.selectedModelId)
        || activeModel;
      setActiveModel(selected || null);
      setRuntimeSettings(normalizeRuntimeSettings(saved, session, selected || null));
    } catch (cause) {
      setRuntimeSettings(previous);
      setError(cause instanceof Error ? cause.message : String(cause));
      throw cause;
    }
  }, [activeModel, auth, conversationId, conversationTitle, modelConfigs, rawSessions, runtimeSettings]);

  const selectContact = useCallback(async (contactId: string) => {
    if (!auth?.accessToken || thinking || contactId === activeContactId) return;
    const contact = contacts.find((item) => item.id === contactId);
    if (!contact) throw new Error('找不到这个联系人');
    setActiveContactId(contact.id);
    setError(null);
    if (contact.sessionId) {
      const session = rawSessions.find((item) => item.id === contact.sessionId)
        || await apiRequest<RawSession>(`/conversations/${encodeURIComponent(contact.sessionId)}`, auth.accessToken);
      if (!rawSessions.some((item) => item.id === session.id)) setRawSessions((current) => [session, ...current]);
      await loadConversationData(auth.accessToken, session, modelConfigs, 40);
      return;
    }
    const fallbackModel = modelConfigs.find((item) => item.enabled !== false) || modelConfigs[0] || null;
    setConversationId(null);
    setConversationTitle(contact.name);
    setPersistedMessages([]);
    setTasks([]);
    setHasMoreMessages(false);
    setActiveModel(fallbackModel);
    setRuntimeSettings((current) => ({
      ...current,
      selectedModelId: current.selectedModelId || fallbackModel?.id || null,
      selectedModelName: current.selectedModelName || fallbackModel?.model_name || fallbackModel?.model || null,
      selectedThinkingLevel: current.selectedThinkingLevel || fallbackModel?.thinking_level || null,
    }));
  }, [activeContactId, auth, contacts, loadConversationData, modelConfigs, rawSessions, thinking]);

  const selectPersonalContacts = useCallback(async () => {
    if (!auth?.accessToken || thinking) return;
    setActiveProjectId(null);
    setRawProjectContacts([]);
    const first = rawContacts[0];
    if (!first) {
      setActiveContactId(null);
      setConversationId(null);
      setConversationTitle(null);
      setPersistedMessages([]);
      return;
    }
    const agentId = value(first.agent_id, first.agentId);
    const session = findContactSession(rawSessions, first.id, agentId, null);
    setActiveContactId(first.id);
    if (session) {
      await loadConversationData(auth.accessToken, session, modelConfigs, 40);
    } else {
      const agent = rawAgents.find((item) => item.id === agentId);
      setConversationId(null);
      setConversationTitle(value(first.agent_name_snapshot, first.agentNameSnapshot) || agent?.name || '联系人');
      setPersistedMessages([]);
      setTasks([]);
      setHasMoreMessages(false);
    }
  }, [auth, loadConversationData, modelConfigs, rawAgents, rawContacts, rawSessions, thinking]);

  const addContact = useCallback(async (agentId: string) => {
    if (!auth?.accessToken || sessionBusy) return null;
    const agent = rawAgents.find((item) => item.id === agentId);
    if (!agent) throw new Error('Agent 不存在或已停用');
    setSessionBusy(true);
    try {
      const result = await apiRequest<RawContact | { contact?: RawContact }>('/contacts', auth.accessToken, {
        method: 'POST',
        body: JSON.stringify({ agent_id: agent.id, agent_name_snapshot: agent.name, user_id: auth.user.id }),
      });
      const created = 'contact' in result && result.contact ? result.contact : result as RawContact;
      setRawContacts((current) => [created, ...current.filter((item) => item.id !== created.id)]);
      setActiveProjectId(null);
      setActiveContactId(created.id);
      setConversationId(null);
      setConversationTitle(agent.name);
      setPersistedMessages([]);
      setRawProjectContacts([]);
      return created.id;
    } finally {
      setSessionBusy(false);
    }
  }, [auth, rawAgents, sessionBusy]);

  const deleteContact = useCallback(async (contactId: string) => {
    if (!auth?.accessToken || sessionBusy || thinking) return;
    setSessionBusy(true);
    try {
      await apiRequest(`/contacts/${encodeURIComponent(contactId)}`, auth.accessToken, { method: 'DELETE' });
      const remaining = rawContacts.filter((item) => item.id !== contactId);
      setRawContacts(remaining);
      if (activeContactId === contactId) {
        setActiveContactId(null);
        setConversationId(null);
        setConversationTitle(null);
        setPersistedMessages([]);
      }
    } finally {
      setSessionBusy(false);
    }
  }, [activeContactId, auth, rawContacts, sessionBusy, thinking]);

  const assignProjectContact = useCallback(async (contactId: string) => {
    if (!auth?.accessToken || !activeProjectId || sessionBusy) return;
    setSessionBusy(true);
    try {
      await apiRequest(`/projects/${encodeURIComponent(activeProjectId)}/contacts`, auth.accessToken, {
        method: 'POST',
        body: JSON.stringify({ contact_id: contactId }),
      });
      await loadProjectContactRows(auth.accessToken, activeProjectId);
      setActiveContactId(contactId);
      setConversationId(null);
      const contact = rawContacts.find((item) => item.id === contactId);
      const agent = rawAgents.find((item) => item.id === value(contact?.agent_id, contact?.agentId));
      setConversationTitle(value(contact?.agent_name_snapshot, contact?.agentNameSnapshot) || agent?.name || '项目负责人');
      setPersistedMessages([]);
    } finally {
      setSessionBusy(false);
    }
  }, [activeProjectId, auth, loadProjectContactRows, rawAgents, rawContacts, sessionBusy]);

  const removeProjectContact = useCallback(async (contactId: string) => {
    if (!auth?.accessToken || !activeProjectId || sessionBusy || thinking) return;
    setSessionBusy(true);
    try {
      await apiRequest(`/projects/${encodeURIComponent(activeProjectId)}/contacts/${encodeURIComponent(contactId)}`, auth.accessToken, { method: 'DELETE' });
      const rows = await loadProjectContactRows(auth.accessToken, activeProjectId);
      if (activeContactId === contactId) {
        const nextContactId = rows[0] ? value(rows[0].contact_id, rows[0].contactId) : '';
        setActiveContactId(nextContactId || null);
        setConversationId(null);
        setConversationTitle(rows[0] ? value(rows[0].agent_name_snapshot, rows[0].agentNameSnapshot) || '项目负责人' : null);
        setPersistedMessages([]);
      }
    } finally {
      setSessionBusy(false);
    }
  }, [activeContactId, activeProjectId, auth, loadProjectContactRows, sessionBusy, thinking]);

  const ensureContactConversation = useCallback(async (): Promise<string> => {
    if (conversationId) return conversationId;
    if (!auth?.accessToken || !activeContactId || !activeModel) throw new Error('请先选择联系人或项目负责人');
    const contact = contacts.find((item) => item.id === activeContactId);
    if (!contact) throw new Error('当前联系人不可用');
    const projectScope = activeProjectId || '0';
    const id = `conv_${typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : Date.now()}`;
    const metadata = {
      chat_runtime: {
        selected_model_id: runtimeSettings.selectedModelId || activeModel.id,
        selected_model_name: runtimeSettings.selectedModelName,
        selected_thinking_level: runtimeSettings.selectedThinkingLevel,
        contact_agent_id: contact.agentId,
        project_id: projectScope,
        reasoning_enabled: runtimeSettings.reasoningEnabled,
        plan_mode_enabled: runtimeSettings.planModeEnabled,
      },
      contact: { type: 'memory_agent', agent_id: contact.agentId, contact_id: contact.id },
      ui_chat_selection: { selected_model_id: runtimeSettings.selectedModelId || activeModel.id, selected_agent_id: contact.agentId },
      ui_contact: { type: 'memory_agent', agent_id: contact.agentId, contact_id: contact.id },
    };
    const created = await apiRequest<RawSession>('/conversations', auth.accessToken, {
      method: 'POST',
      body: JSON.stringify({ id, title: contact.name, user_id: auth.user.id, project_id: projectScope, metadata }),
    });
    setRawSessions((current) => [created, ...current.filter((item) => item.id !== created.id)]);
    setConversationId(created.id);
    setConversationTitle(contact.name);
    return created.id;
  }, [activeContactId, activeModel, activeProjectId, auth, contacts, conversationId, runtimeSettings]);

  const selectProject = useCallback(async (projectId: string) => {
    if (!auth?.accessToken) return;
    setActiveProjectId(projectId);
    const projectContactRows = await loadProjectContactRows(auth.accessToken, projectId);
    const rawProject = rawProjects.find((project) => project.id === projectId);
    if (rawProject) {
      const plan = await apiRequest<RawProjectPlan>(`/projects/${encodeURIComponent(projectId)}/plan?include_work_items=true`, auth.accessToken)
        .catch(() => null);
      setProjects((current) => current.map((project, index) => (
        project.id === projectId ? mapProject(rawProject, index, plan) : project
      )));
    }
    const preferredSessionId = projectContactRows.map((item) => value(item.latest_session_id, item.latestSessionId)).find(Boolean)
      || (rawProject ? value(rawProject.latest_session_id, rawProject.latestSessionId) : '');
    const session = rawSessions.find((item) => item.id === preferredSessionId)
      || latestSession(rawSessions.filter((item) => value(item.project_id, item.projectId) === projectId));
    if (session) {
      await loadConversationData(auth.accessToken, session, modelConfigs);
      const matchedContact = projectContactRows.find((item) => value(item.latest_session_id, item.latestSessionId) === session.id)
        || projectContactRows.find((item) => value(item.agent_id, item.agentId) === sessionIdentity(session).agentId);
      if (matchedContact) setActiveContactId(value(matchedContact.contact_id, matchedContact.contactId) || null);
    } else {
      const firstContact = projectContactRows[0];
      setActiveContactId(firstContact ? value(firstContact.contact_id, firstContact.contactId) || null : null);
      setConversationId(null);
      setConversationTitle(firstContact ? value(firstContact.agent_name_snapshot, firstContact.agentNameSnapshot) || '项目负责人' : null);
      setPersistedMessages([]);
      setTasks([]);
      setHasMoreMessages(false);
    }
  }, [auth, loadConversationData, loadProjectContactRows, modelConfigs, rawProjects, rawSessions]);

  const sendMessage = useCallback(async (content: string, files: File[] = []) => {
    const normalized = content.trim();
    if ((!normalized && files.length === 0) || !auth?.accessToken || !activeModel) {
      throw new Error('真实聊天尚未准备好：需要联系人和模型配置');
    }
    const targetConversationId = await ensureContactConversation();
    const optimisticId = `optimistic-${Date.now()}`;
    const userMessage: ChatMessage = {
      id: optimisticId,
      role: 'user',
      content: normalized,
      time: formatMessageTime(),
      status: 'sending',
      attachments: files.map((file, index) => ({
        id: `${optimisticId}-${index}`,
        name: file.name,
        mimeType: file.type || 'application/octet-stream',
        size: file.size,
        type: file.type.startsWith('image/') ? 'image' : file.type.startsWith('audio/') ? 'audio' : 'file',
      })),
    };
    setPersistedMessages((current) => [...current, userMessage]);
    setStreamingText('');
    setThinking(true);
    setError(null);
    const activeProject = rawProjects.find((project) => project.id === activeProjectId);
    const projectRoot = activeProject
      ? value(activeProject.display_root_path, activeProject.displayRootPath) || value(activeProject.root_path, activeProject.rootPath)
      : '';
    const turnId = `turn_${typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : Date.now()}`;
    activeTurnIdRef.current = turnId;
    try {
      const attachments = await prepareAttachmentPayloads(files);
      await apiRequest('/agent/chat/send', auth.accessToken, {
        method: 'POST',
        body: JSON.stringify({
          conversation_id: targetConversationId,
          content: normalized,
          user_id: auth.user.id,
          attachments,
          reasoning_enabled: runtimeSettings.reasoningEnabled,
          turn_id: turnId,
          project_id: activeProjectId || undefined,
          project_root: projectRoot || undefined,
          workspace_root: projectRoot || undefined,
          plan_mode: runtimeSettings.planModeEnabled,
          model_config_id: activeModel.id,
          ai_model_config: {
            temperature: activeModel.temperature ?? 0.7,
            model_name: activeModel.model_name || activeModel.model || '',
            thinking_level: runtimeSettings.selectedThinkingLevel || activeModel.thinking_level || null,
          },
        }),
      });
      setPersistedMessages((current) => current.map((message) => (
        message.id === optimisticId ? { ...message, status: 'complete' } : message
      )));
    } catch (cause) {
      setThinking(false);
      activeTurnIdRef.current = null;
      setPersistedMessages((current) => current.map((message) => (
        message.id === optimisticId ? { ...message, status: 'error' } : message
      )));
      setError(cause instanceof Error ? cause.message : String(cause));
      throw cause;
    }
  }, [activeModel, activeProjectId, auth, ensureContactConversation, rawProjects, runtimeSettings.planModeEnabled, runtimeSettings.reasoningEnabled, runtimeSettings.selectedThinkingLevel]);

  const stopMessage = useCallback(async () => {
    if (!auth?.accessToken || !conversationId || (!thinking && !activeTurnIdRef.current) || isStopping) return;
    setIsStopping(true);
    setError(null);
    try {
      await apiRequest('/agent/chat/stop', auth.accessToken, {
        method: 'POST',
        body: JSON.stringify({
          conversation_id: conversationId,
          turn_id: activeTurnIdRef.current || undefined,
        }),
      });
      setThinking(false);
      activeTurnIdRef.current = null;
      scheduleConversationRefresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      throw cause;
    } finally {
      setIsStopping(false);
    }
  }, [auth, conversationId, isStopping, scheduleConversationRefresh, thinking]);

  return {
    apiBaseUrl: API_BASE_URL,
    status,
    webSocketStatus,
    error,
    user: auth?.user || null,
    projects,
    tasks,
    runningTasks,
    taskGraph,
    taskGraphTaskId,
    taskGraphLoading,
    taskGraphError,
    messages,
    models,
    contacts,
    accountContacts,
    agents,
    availableAgents,
    runtimeSettings,
    activeProjectId,
    activeContactId,
    thinking,
    isStopping,
    loadingMessages,
    hasMoreMessages,
    sessionBusy,
    conversationId,
    conversationTitle,
    login,
    logout,
    refresh,
    loadTaskGraph,
    selectContact,
    selectPersonalContacts,
    addContact,
    deleteContact,
    assignProjectContact,
    removeProjectContact,
    loadMoreMessages,
    updateRuntimeSettings,
    selectProject,
    sendMessage,
    stopMessage,
  };
}
