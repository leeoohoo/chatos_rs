import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useChatApiClientFromContext, useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { MessageList } from './MessageList';
import { InputArea } from './InputArea';
import { SessionList } from './SessionList';
import { ThemeToggle } from './ThemeToggle';
import McpManager from './McpManager';
import AiModelManager from './AiModelManager';
import SystemContextEditor from './SystemContextEditor';
import AgentManager from './AgentManager';
import UserSettingsPanel from './UserSettingsPanel';
import ProjectExplorer from './ProjectExplorer';
import TerminalView from './TerminalView';
// 搴旂敤寮圭獥绠＄悊鍣ㄧ敱 ApplicationsPanel 鐩存帴鎵挎媴
import ApplicationsPanel from './ApplicationsPanel';
import TaskDraftPanel from './TaskDraftPanel';
import TaskWorkbar, { type TaskWorkbarItem } from './TaskWorkbar';
import ApiClient from '../lib/api/client';
import { cn } from '../lib/utils';
import type { ChatInterfaceProps } from '../types';
import type { TaskReviewDraft } from '../lib/store/types';

const SESSION_PAGE_SIZE = 30;

export const ChatInterface: React.FC<ChatInterfaceProps> = ({
  className,
  onMessageSend,
  customRenderer,
}) => {
  const {
    currentSession,
    currentProject,
    currentTerminal,
    projects,
    activePanel,
    messages,
    hasMoreMessages,
    error,
    loadSessions,
    loadProjects,
    // selectSession,
    loadMoreMessages,
    sendMessage,
    clearError,
    sidebarOpen,
    toggleSidebar,
    aiModelConfigs,
    selectedModelId,
    setSelectedModel,
    loadAiModelConfigs,
    agents,
    selectedAgentId,
    setSelectedAgent,
    loadAgents,
    chatConfig,
    updateChatConfig,
    abortCurrentConversation,
    sessionChatState = {},
    taskReviewPanelsBySession = {},
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    // applications,  // 涓嶅啀鍦ㄦ缁勪欢涓娇鐢?
    // selectedApplicationId,  // 涓嶅啀鐢ㄤ簬鑷姩鏄剧ず
  } = useChatStoreFromContext();

  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = useMemo(() => apiClientFromContext || new ApiClient(), [apiClientFromContext]);

  const selectedAgent = useMemo(
    () => (selectedAgentId ? agents.find((a: any) => a.id === selectedAgentId) : null),
    [agents, selectedAgentId]
  );
  const activeModelConfig = useMemo(() => (
    selectedAgent
      ? aiModelConfigs.find((m: any) => m.id === selectedAgent.ai_model_config_id)
      : aiModelConfigs.find((m: any) => m.id === selectedModelId)
  ), [aiModelConfigs, selectedAgent, selectedModelId]);
  const supportsImages = activeModelConfig?.supports_images === true;
  const supportsReasoning = activeModelConfig?.supports_reasoning === true;
  const supportedFileTypes = useMemo(() => (
    supportsImages
      ? ['image/*', 'text/*', 'application/json', 'application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']
      : ['text/*', 'application/json', 'application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']
  ), [supportsImages]);
  const currentChatState = useMemo(() => (
    currentSession ? sessionChatState[currentSession.id] : undefined
  ), [currentSession, sessionChatState]);
  const chatIsLoading = currentChatState?.isLoading ?? false;
  const chatIsStreaming = currentChatState?.isStreaming ?? false;
  const headerTitle = activePanel === 'project'
    ? (currentProject?.name || '项目')
    : activePanel === 'terminal'
      ? (currentTerminal?.name || '终端')
      : (currentSession?.title || '');

  const [showMcpManager, setShowMcpManager] = useState(false);
  const [showAiModelManager, setShowAiModelManager] = useState(false);
  const [showSystemContextEditor, setShowSystemContextEditor] = useState(false);
  const [showAgentManager, setShowAgentManager] = useState(false);
  const [showApplicationsPanel, setShowApplicationsPanel] = useState(false);
  const [showUserSettings, setShowUserSettings] = useState(false);
  const didInitRef = useRef(false);
  const [workbarCurrentTurnTasks, setWorkbarCurrentTurnTasks] = useState<TaskWorkbarItem[]>([]);
  const [workbarHistoryTasks, setWorkbarHistoryTasks] = useState<TaskWorkbarItem[]>([]);
  const [workbarHistoryLoadedSessionId, setWorkbarHistoryLoadedSessionId] = useState<string | null>(null);
  const [workbarLoading, setWorkbarLoading] = useState(false);
  const [workbarHistoryLoading, setWorkbarHistoryLoading] = useState(false);
  const [workbarError, setWorkbarError] = useState<string | null>(null);
  const [workbarHistoryError, setWorkbarHistoryError] = useState<string | null>(null);
  const [workbarActionLoadingTaskId, setWorkbarActionLoadingTaskId] = useState<string | null>(null);

  const activeTaskReviewPanel = useMemo(() => {
    if (!currentSession) {
      return null;
    }
    const panels = taskReviewPanelsBySession[currentSession.id];
    if (!Array.isArray(panels) || panels.length === 0) {
      return null;
    }
    return panels[0];
  }, [currentSession, taskReviewPanelsBySession]);

  const activeConversationTurnId = useMemo(() => {
    if (!currentSession) {
      return null;
    }

    for (let i = messages.length - 1; i >= 0; i -= 1) {
      const message = messages[i] as any;
      if (message?.sessionId && message.sessionId !== currentSession.id) {
        continue;
      }
      const turnId = typeof message?.metadata?.conversation_turn_id === 'string'
        ? message.metadata.conversation_turn_id.trim()
        : '';
      if (turnId) {
        return turnId;
      }
    }

    return null;
  }, [currentSession, messages]);

  const handledTaskMutationKeysRef = useRef<Set<string>>(new Set());

  const isTaskMutationToolName = useCallback((name: unknown) => {
    const normalized = String(name || '').toLowerCase();
    if (!normalized) {
      return false;
    }

    const taskScope = normalized.includes('task_manager') || normalized.includes('task');
    if (!taskScope) {
      return false;
    }

    return normalized.includes('add_task')
      || normalized.includes('update_task')
      || normalized.includes('complete_task')
      || normalized.includes('delete_task');
  }, []);

  const collectMessageToolCalls = useCallback((message: any) => {
    const topLevel = Array.isArray(message?.toolCalls) ? message.toolCalls : [];
    const metadataLevel = Array.isArray(message?.metadata?.toolCalls)
      ? message.metadata.toolCalls
      : [];

    const merged = [...metadataLevel, ...topLevel];
    if (merged.length <= 1) {
      return merged;
    }

    const seen = new Set<string>();
    return merged.filter((toolCall: any, index: number) => {
      const key = String(
        toolCall?.id || toolCall?.tool_call_id || toolCall?.toolCallId || `${index}:${toolCall?.name || ''}`
      );
      if (seen.has(key)) {
        return false;
      }
      seen.add(key);
      return true;
    });
  }, []);

  const shouldRefreshForTaskMutationToolCall = useCallback((toolCall: any) => {
    if (isTaskMutationToolName(toolCall?.name)) {
      return true;
    }

    const normalizedName = String(toolCall?.name || '').toLowerCase();
    if (!normalizedName.includes('sub_agent')) {
      return false;
    }

    const combinedOutput = [toolCall?.result, toolCall?.finalResult, toolCall?.streamLog]
      .filter((value) => typeof value === 'string' && value.trim())
      .map((value) => String(value).toLowerCase())
      .join(' ');

    if (!combinedOutput) {
      return true;
    }

    return combinedOutput.includes('task_manager_builtin__')
      || combinedOutput.includes('task_manager')
      || combinedOutput.includes('add_task')
      || combinedOutput.includes('update_task')
      || combinedOutput.includes('complete_task')
      || combinedOutput.includes('delete_task');
  }, [isTaskMutationToolName]);

  const hasToolCallError = useCallback((toolCall: any) => {
    if (toolCall?.error === null || toolCall?.error === undefined) {
      return false;
    }
    if (typeof toolCall.error === 'string') {
      return toolCall.error.trim().length > 0;
    }
    return true;
  }, []);

  const CURRENT_TURN_MUTATION_FALLBACK_LIMIT = 8;

  const parseMaybeJsonValue = useCallback((value: unknown) => {
    if (typeof value !== 'string') {
      return value;
    }

    const trimmed = value.trim();
    if (!trimmed) {
      return null;
    }

    try {
      return JSON.parse(trimmed);
    } catch (_) {
      return null;
    }
  }, []);

  const collectTaskIdsFromToolResult = useCallback((value: unknown, collector: Set<string>, depth = 0) => {
    if (!value || depth > 5) {
      return;
    }

    if (Array.isArray(value)) {
      value.forEach((item) => collectTaskIdsFromToolResult(item, collector, depth + 1));
      return;
    }

    if (typeof value !== 'object') {
      return;
    }

    const record = value as Record<string, unknown>;

    const taskId = typeof record.task_id === 'string' ? record.task_id.trim() : '';
    if (taskId) {
      collector.add(taskId);
    }

    if (record.task && typeof record.task === 'object') {
      const nestedTask = record.task as Record<string, unknown>;
      const nestedId = typeof nestedTask.id === 'string' ? nestedTask.id.trim() : '';
      if (nestedId) {
        collector.add(nestedId);
      }
      collectTaskIdsFromToolResult(record.task, collector, depth + 1);
    }

    if (Array.isArray(record.tasks)) {
      record.tasks.forEach((task) => {
        if (task && typeof task === 'object') {
          const taskIdValue = typeof (task as Record<string, unknown>).id === 'string'
            ? (task as Record<string, unknown>).id as string
            : '';
          if (taskIdValue.trim()) {
            collector.add(taskIdValue.trim());
          }
        }
      });
      collectTaskIdsFromToolResult(record.tasks, collector, depth + 1);
    }

    const looksLikeTask = typeof record.id === 'string'
      && (typeof record.title === 'string' || typeof record.status === 'string');
    if (looksLikeTask) {
      collector.add((record.id as string).trim());
    }

    Object.values(record).forEach((child) => collectTaskIdsFromToolResult(child, collector, depth + 1));
  }, []);

  const extractTaskIdsFromToolCall = useCallback((toolCall: any) => {
    const output = new Set<string>();

    const candidates = [
      toolCall?.result,
      toolCall?.finalResult,
      parseMaybeJsonValue(toolCall?.result),
      parseMaybeJsonValue(toolCall?.finalResult),
    ];

    candidates.forEach((item) => collectTaskIdsFromToolResult(item, output));

    return Array.from(output);
  }, [collectTaskIdsFromToolResult, parseMaybeJsonValue]);

  const normalizeWorkbarTask = useCallback((raw: any): TaskWorkbarItem => {
    const statusRaw = String(raw?.status || 'todo').toLowerCase();
    const status: TaskWorkbarItem['status'] =
      statusRaw === 'doing' || statusRaw === 'blocked' || statusRaw === 'done'
        ? statusRaw
        : 'todo';

    const priorityRaw = String(raw?.priority || 'medium').toLowerCase();
    const priority: TaskWorkbarItem['priority'] =
      priorityRaw === 'high' || priorityRaw === 'low' ? priorityRaw : 'medium';

    const conversationTurnId = String(raw?.conversation_turn_id ?? raw?.conversationTurnId ?? '').trim();
    const createdAt = String(raw?.created_at ?? raw?.createdAt ?? '');
    const dueAtRaw = raw?.due_at ?? raw?.dueAt;

    return {
      id: String(raw?.id || '').trim(),
      title: String(raw?.title || ''),
      details: String(raw?.details || raw?.description || ''),
      status,
      priority,
      conversationTurnId,
      createdAt,
      dueAt: dueAtRaw ? String(dueAtRaw) : null,
      tags: Array.isArray(raw?.tags)
        ? raw.tags
            .map((tag: any) => String(tag).trim())
            .filter((tag: string) => tag.length > 0)
        : [],
    };
  }, []);

  const selectLatestTurnTasks = useCallback((tasks: TaskWorkbarItem[]) => {
    if (tasks.length === 0) {
      return [];
    }

    const latestTaskWithTurn = tasks.find((task) => task.conversationTurnId.trim().length > 0);
    if (!latestTaskWithTurn) {
      return tasks.slice(0, 8);
    }

    const latestTurnId = latestTaskWithTurn.conversationTurnId.trim();
    return tasks.filter((task) => task.conversationTurnId.trim() === latestTurnId);
  }, []);

  const currentTurnMutationTaskIds = useMemo(() => {
    if (!currentSession || !activeConversationTurnId) {
      return [];
    }

    const ids = new Set<string>();
    let lastKnownTurnId = '';

    for (const message of messages as any[]) {
      if (message?.sessionId && message.sessionId !== currentSession.id) {
        continue;
      }

      const messageTurnId = typeof message?.metadata?.conversation_turn_id === 'string'
        ? message.metadata.conversation_turn_id.trim()
        : '';
      if (messageTurnId) {
        lastKnownTurnId = messageTurnId;
      }

      const effectiveTurnId = messageTurnId || lastKnownTurnId;
      if (effectiveTurnId !== activeConversationTurnId) {
        continue;
      }

      const toolCalls = collectMessageToolCalls(message);
      for (const toolCall of toolCalls) {
        if (!shouldRefreshForTaskMutationToolCall(toolCall)) {
          continue;
        }
        if (toolCall?.completed !== true || hasToolCallError(toolCall)) {
          continue;
        }

        extractTaskIdsFromToolCall(toolCall).forEach((taskId) => ids.add(taskId));
      }
    }

    return Array.from(ids);
  }, [
    activeConversationTurnId,
    collectMessageToolCalls,
    currentSession,
    extractTaskIdsFromToolCall,
    hasToolCallError,
    messages,
    shouldRefreshForTaskMutationToolCall,
  ]);

  const mergedCurrentTurnTasks = useMemo(() => {
    const baseTasks = workbarCurrentTurnTasks.length > 0
      ? workbarCurrentTurnTasks
      : selectLatestTurnTasks(workbarHistoryTasks);

    if (currentTurnMutationTaskIds.length === 0) {
      return baseTasks;
    }

    const existing = new Set(baseTasks.map((task) => task.id));
    const fallbackCandidates = workbarHistoryTasks
      .filter((task) => currentTurnMutationTaskIds.includes(task.id) && !existing.has(task.id))
      .slice(0, CURRENT_TURN_MUTATION_FALLBACK_LIMIT);

    if (fallbackCandidates.length === 0) {
      return baseTasks;
    }

    return [...baseTasks, ...fallbackCandidates];
  }, [currentTurnMutationTaskIds, selectLatestTurnTasks, workbarCurrentTurnTasks, workbarHistoryTasks]);

  const loadCurrentTurnWorkbarTasks = useCallback(async (sessionId: string, conversationTurnId?: string | null) => {
    if (!sessionId) {
      setWorkbarCurrentTurnTasks([]);
      setWorkbarError(null);
      return;
    }

    const turnId = typeof conversationTurnId === 'string' ? conversationTurnId.trim() : '';

    setWorkbarLoading(true);
    setWorkbarError(null);
    try {
      let normalizedTasks: TaskWorkbarItem[] = [];

      if (turnId) {
        const tasks = await apiClient.getTaskManagerTasks(sessionId, {
          conversationTurnId: turnId,
          includeDone: true,
          limit: 100,
        });
        normalizedTasks = tasks.map(normalizeWorkbarTask);
      }

      if (normalizedTasks.length === 0) {
        const fallbackTasks = await apiClient.getTaskManagerTasks(sessionId, {
          includeDone: true,
          limit: 200,
        });
        normalizedTasks = selectLatestTurnTasks(fallbackTasks.map(normalizeWorkbarTask));
      }

      setWorkbarCurrentTurnTasks(normalizedTasks);
    } catch (error) {
      setWorkbarError(error instanceof Error ? error.message : '任务操作失败');
    } finally {
      setWorkbarLoading(false);
    }
  }, [apiClient, normalizeWorkbarTask, selectLatestTurnTasks]);

  const loadHistoryWorkbarTasks = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId) {
      setWorkbarHistoryTasks([]);
      setWorkbarHistoryError(null);
      setWorkbarHistoryLoadedSessionId(null);
      return;
    }

    if (!force && workbarHistoryLoadedSessionId === sessionId && workbarHistoryTasks.length > 0) {
      return;
    }

    setWorkbarHistoryLoading(true);
    setWorkbarHistoryError(null);
    try {
      const tasks = await apiClient.getTaskManagerTasks(sessionId, {
        includeDone: true,
        limit: 300,
      });
      setWorkbarHistoryTasks(tasks.map(normalizeWorkbarTask));
      setWorkbarHistoryLoadedSessionId(sessionId);
    } catch (error) {
      setWorkbarHistoryError(error instanceof Error ? error.message : '\u4efb\u52a1\u52a0\u8f7d\u5931\u8d25');
    } finally {
      setWorkbarHistoryLoading(false);
    }
  }, [apiClient, normalizeWorkbarTask, workbarHistoryLoadedSessionId, workbarHistoryTasks.length]);

  const refreshWorkbarTasks = useCallback(async () => {
    if (!currentSession) {
      return;
    }
    await Promise.all([
      loadCurrentTurnWorkbarTasks(currentSession.id, activeConversationTurnId),
      loadHistoryWorkbarTasks(currentSession.id, true),
    ]);
  }, [activeConversationTurnId, currentSession, loadCurrentTurnWorkbarTasks, loadHistoryWorkbarTasks]);

  useEffect(() => {
    handledTaskMutationKeysRef.current.clear();
  }, [currentSession?.id]);

  useEffect(() => {
    if (!currentSession || activePanel !== 'chat') {
      return;
    }

    const handled = handledTaskMutationKeysRef.current;
    const pendingKeys: string[] = [];

    for (const message of messages as any[]) {
      if (message?.sessionId && message.sessionId !== currentSession.id) {
        continue;
      }

      const toolCalls = collectMessageToolCalls(message);
      for (const toolCall of toolCalls) {
        if (!shouldRefreshForTaskMutationToolCall(toolCall)) {
          continue;
        }
        if (toolCall?.completed !== true) {
          continue;
        }
        if (hasToolCallError(toolCall)) {
          continue;
        }

        const toolCallId = String(toolCall?.id || toolCall?.tool_call_id || toolCall?.toolCallId || '').trim();
        const key = currentSession.id + ':' + String(message?.id || '') + ':' + (toolCallId || String(toolCall?.name || ''));
        if (handled.has(key)) {
          continue;
        }
        pendingKeys.push(key);
      }
    }

    if (pendingKeys.length === 0) {
      return;
    }

    pendingKeys.forEach((key) => handled.add(key));
    void refreshWorkbarTasks();
  }, [
    activePanel,
    collectMessageToolCalls,
    currentSession,
    hasToolCallError,
    messages,
    refreshWorkbarTasks,
    shouldRefreshForTaskMutationToolCall,
  ]);

  const withWorkbarTaskMutation = useCallback(async (taskId: string, action: () => Promise<void>) => {
    setWorkbarActionLoadingTaskId(taskId);
    setWorkbarError(null);
    try {
      await action();
      await refreshWorkbarTasks();
    } catch (error) {
      setWorkbarError(error instanceof Error ? error.message : '任务操作失败');
    } finally {
      setWorkbarActionLoadingTaskId(null);
    }
  }, [refreshWorkbarTasks]);

  const handleWorkbarCompleteTask = useCallback(async (task: TaskWorkbarItem) => {
    if (!currentSession) {
      return;
    }
    await withWorkbarTaskMutation(task.id, async () => {
      await apiClient.completeTaskManagerTask(currentSession.id, task.id);
    });
  }, [apiClient, currentSession, withWorkbarTaskMutation]);

  const handleWorkbarDeleteTask = useCallback(async (task: TaskWorkbarItem) => {
    if (!currentSession) {
      return;
    }
    if (typeof window !== 'undefined') {
      const confirmed = window.confirm('Delete task "' + task.title + '"?');
      if (!confirmed) {
        return;
      }
    }

    await withWorkbarTaskMutation(task.id, async () => {
      await apiClient.deleteTaskManagerTask(currentSession.id, task.id);
    });
  }, [apiClient, currentSession, withWorkbarTaskMutation]);

  const handleWorkbarEditTask = useCallback(async (task: TaskWorkbarItem) => {
    if (!currentSession || typeof window === 'undefined') {
      return;
    }

    const nextTitleRaw = window.prompt('Task title', task.title);
    if (nextTitleRaw === null) {
      return;
    }
    const nextDetailsRaw = window.prompt('Task details (optional)', task.details || '');
    if (nextDetailsRaw === null) {
      return;
    }
    const nextPriorityRaw = window.prompt('Priority (high/medium/low)', task.priority);
    if (nextPriorityRaw === null) {
      return;
    }
    const nextStatusRaw = window.prompt('Status (todo/doing/blocked/done)', task.status);
    if (nextStatusRaw === null) {
      return;
    }
    const nextDueAtRaw = window.prompt('Due time (empty string to clear)', task.dueAt || '');
    if (nextDueAtRaw === null) {
      return;
    }

    const allowedPriority: Array<TaskWorkbarItem['priority']> = ['high', 'medium', 'low'];
    const allowedStatus: Array<TaskWorkbarItem['status']> = ['todo', 'doing', 'blocked', 'done'];
    const nextPriority = nextPriorityRaw.trim().toLowerCase() as TaskWorkbarItem['priority'];
    const nextStatus = nextStatusRaw.trim().toLowerCase() as TaskWorkbarItem['status'];

    if (!allowedPriority.includes(nextPriority)) {
      setWorkbarError('Priority must be high / medium / low');
      return;
    }
    if (!allowedStatus.includes(nextStatus)) {
      setWorkbarError('Status must be todo / doing / blocked / done');
      return;
    }

    const nextTitle = nextTitleRaw.trim();
    const nextDetails = nextDetailsRaw.trim();
    const nextDueAt = nextDueAtRaw.trim();

    const payload: {
      title?: string;
      details?: string;
      priority?: TaskWorkbarItem['priority'];
      status?: TaskWorkbarItem['status'];
      due_at?: string | null;
    } = {};

    if (nextTitle && nextTitle !== task.title) {
      payload.title = nextTitle;
    }
    if (nextDetails !== task.details) {
      payload.details = nextDetails;
    }
    if (nextPriority !== task.priority) {
      payload.priority = nextPriority;
    }
    if (nextStatus !== task.status) {
      payload.status = nextStatus;
    }

    const currentDueAt = (task.dueAt || '').trim();
    if (nextDueAt !== currentDueAt) {
      payload.due_at = nextDueAt || null;
    }

    if (Object.keys(payload).length === 0) {
      return;
    }

    await withWorkbarTaskMutation(task.id, async () => {
      await apiClient.updateTaskManagerTask(currentSession.id, task.id, payload);
    });
  }, [apiClient, currentSession, withWorkbarTaskMutation]);

  // 鍒濆鍖栧姞杞戒細璇濄€丄I妯″瀷鍜屾櫤鑳戒綋閰嶇疆
  useEffect(() => {
    // React 18 鍦ㄥ紑鍙戞ā寮忎笅浼氬弻璋冪敤鍓綔鐢紝杩欓噷鍔犱竴娆℃€т繚鎶わ紙缁勪欢鍐咃級
    if (didInitRef.current) return;
    didInitRef.current = true;

    loadSessions({ limit: SESSION_PAGE_SIZE, offset: 0 });
    loadProjects();
    loadAiModelConfigs();
    loadAgents();
  }, [loadSessions, loadProjects, loadAiModelConfigs, loadAgents]);

  useEffect(() => {
    if (!currentSession || activePanel !== 'chat') {
      setWorkbarCurrentTurnTasks([]);
      setWorkbarHistoryTasks([]);
      setWorkbarError(null);
      setWorkbarHistoryError(null);
      setWorkbarHistoryLoadedSessionId(null);
      return;
    }

    void loadCurrentTurnWorkbarTasks(currentSession.id, activeConversationTurnId);
    void loadHistoryWorkbarTasks(currentSession.id);
  }, [activeConversationTurnId, activePanel, currentSession, loadCurrentTurnWorkbarTasks, loadHistoryWorkbarTasks]);

  // 澶勭悊娑堟伅鍙戦€?
  const handleMessageSend = useCallback(async (content: string, attachments?: File[]) => {
    try {
      await sendMessage(content, attachments);
      onMessageSend?.(content, attachments);
    } catch (error) {
      console.error('Failed to send message:', error);
    }
  }, [onMessageSend, sendMessage]);

  const handleLoadMore = useCallback(() => {
    if (currentSession) {
      loadMoreMessages(currentSession.id);
    }
  }, [currentSession, loadMoreMessages]);

  const handleTaskReviewConfirm = useCallback(async (drafts: TaskReviewDraft[]) => {
    if (!activeTaskReviewPanel) {
      return;
    }

    const pendingPanel = {
      ...activeTaskReviewPanel,
      drafts,
      submitting: true,
      error: null,
    };
    upsertTaskReviewPanel(pendingPanel);

    try {
      await apiClient.submitTaskReviewDecision(activeTaskReviewPanel.reviewId, {
        action: 'confirm',
        tasks: drafts.map((draft) => ({
          title: draft.title,
          details: draft.details,
          priority: draft.priority,
          status: draft.status,
          tags: draft.tags,
          due_at: draft.dueAt || undefined,
        })),
      });
      removeTaskReviewPanel(activeTaskReviewPanel.reviewId, activeTaskReviewPanel.sessionId);
      await Promise.all([
        loadCurrentTurnWorkbarTasks(activeTaskReviewPanel.sessionId, activeTaskReviewPanel.conversationTurnId),
        loadHistoryWorkbarTasks(activeTaskReviewPanel.sessionId, true),
      ]);
    } catch (error) {
      const message = error instanceof Error ? error.message : '任务确认提交失败';
      upsertTaskReviewPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [activeTaskReviewPanel, apiClient, loadCurrentTurnWorkbarTasks, loadHistoryWorkbarTasks, removeTaskReviewPanel, upsertTaskReviewPanel]);

  const handleTaskReviewCancel = useCallback(async () => {
    if (!activeTaskReviewPanel) {
      return;
    }

    const pendingPanel = {
      ...activeTaskReviewPanel,
      submitting: true,
      error: null,
    };
    upsertTaskReviewPanel(pendingPanel);

    try {
      await apiClient.submitTaskReviewDecision(activeTaskReviewPanel.reviewId, {
        action: 'cancel',
        reason: 'user_cancelled',
      });
      removeTaskReviewPanel(activeTaskReviewPanel.reviewId, activeTaskReviewPanel.sessionId);
      await Promise.all([
        loadCurrentTurnWorkbarTasks(activeTaskReviewPanel.sessionId, activeTaskReviewPanel.conversationTurnId),
        loadHistoryWorkbarTasks(activeTaskReviewPanel.sessionId, true),
      ]);
    } catch (error) {
      const message = error instanceof Error ? error.message : '任务取消提交失败';
      upsertTaskReviewPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [activeTaskReviewPanel, apiClient, loadCurrentTurnWorkbarTasks, loadHistoryWorkbarTasks, removeTaskReviewPanel, upsertTaskReviewPanel]);


  if (showSystemContextEditor) {
    return (
      <SystemContextEditor onClose={() => setShowSystemContextEditor(false)} />
    );
  }

  return (
    <div className={cn(
      'flex flex-col h-screen bg-background text-foreground',
      className
    )}>
      {/* 澶撮儴 - 鍖呭惈浼氳瘽鎸夐挳鍜屼富棰樺垏鎹?*/}
      <div className="flex items-center justify-between p-4 bg-card border-b border-border">
        <div className="flex items-center space-x-3">
          <button
            onClick={toggleSidebar}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title={sidebarOpen ? '收起会话列表' : '展开会话列表'}
          >
            <svg className={`w-5 h-5 transition-transform ${sidebarOpen ? '' : 'rotate-180'}`} fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M15 18L9 12l6-6" />
            </svg>
          </button>
          
          {headerTitle && (
            <div className="flex-1 min-w-0">
              <h1 className="text-lg font-semibold text-foreground truncate">
                {headerTitle}
              </h1>
            </div>
          )}
        </div>
        
        <div className="flex items-center space-x-2">
          <button
            onClick={() => setShowApplicationsPanel(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="打开应用列表"
          >
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path d="M4 5h6v14H4z" strokeWidth="2" />
              <path d="M12 5h8v14h-8z" strokeWidth="2" />
            </svg>
          </button>
          <button
            onClick={() => setShowMcpManager(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="MCP 服务管理"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
            </svg>
          </button>
          <button
            onClick={() => setShowAgentManager(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="智能体管理"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6M9 16h6M6 8h12a2 2 0 012 2v8a2 2 0 01-2 2H6a2 2 0 01-2-2v-8a2 2 0 012-2z" />
            </svg>
          </button>
          <button
            onClick={() => setShowAiModelManager(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="AI 模型管理"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
            </svg>
          </button>
          <button
            onClick={() => setShowSystemContextEditor(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="系统上下文设置"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
          </button>
          <ThemeToggle />
          {/* 璁剧疆鎸夐挳鏀惧埌鏈€鍙充晶 */}
          <button
            onClick={() => setShowUserSettings(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="用户参数设置"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6V4m0 16v-2m8-6h2M4 12H2m15.364 5.364l1.414 1.414M5.636 6.636L4.222 5.222m12.728 0l1.414 1.414M5.636 17.364l-1.414 1.414" />
            </svg>
          </button>
        </div>
      </div>

          {/* 閿欒鎻愮ず */}
          {error && (
            <div className="mx-4 mt-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg">
              <div className="flex items-center justify-between">
                <p className="text-sm text-destructive">{error}</p>
                <button
                  onClick={clearError}
                  className="text-destructive hover:text-destructive/80 transition-colors"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </div>
          )}

        {/* 涓诲尯鍩燂細宸︿晶浼氳瘽鍒楄〃 + 鍙充晶鑱婂ぉ */}
        <div className="flex flex-1 overflow-hidden">
          <SessionList
            collapsed={!sidebarOpen}
            onToggleCollapse={toggleSidebar}
          />

          {/* 宸茬Щ闄ゅ乏渚у簲鐢ㄦ娊灞夐潰鏉匡紝鏀逛负寮圭獥 */}
          {/* 宓屽叆鍖哄煙宸茬Щ闄?- 搴旂敤閫夋嫨鍚庡彧瑙﹀彂浜嬩欢锛屼笉鑷姩鏄剧ず */}
          {/* 澶栭儴鍙互閫氳繃 subscribeSelectedApplication 鐩戝惉搴旂敤閫夋嫨浜嬩欢 */}
          {/* 鐒跺悗鑷鍐冲畾濡備綍鎵撳紑/鏄剧ず搴旂敤锛圗lectron 绐楀彛銆亀indow.open 绛夛級 */}

          <div className="flex-1 flex flex-col overflow-hidden">
            {activePanel === 'project' ? (
              <ProjectExplorer project={currentProject} className="flex-1" />
            ) : activePanel === 'terminal' ? (
              <TerminalView className="flex-1" />
            ) : (
              <div className="flex-1 flex overflow-hidden">
                <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
                  <div className="flex-1 overflow-hidden">
                    {currentSession ? (
                      <MessageList
                        messages={messages}
                        isLoading={chatIsLoading}
                        isStreaming={chatIsStreaming}
                        hasMore={hasMoreMessages}
                        onLoadMore={handleLoadMore}
                        customRenderer={customRenderer}
                      />
                    ) : (
                      <div className="flex items-center justify-center h-full">
                        <div className="text-center">
                          <h2 className="text-xl font-semibold text-muted-foreground mb-2">
                            娆㈣繋浣跨敤 AI 鑱婂ぉ
                          </h2>
                          <p className="text-muted-foreground mb-4">
                            鐐瑰嚮宸︿笂瑙掓寜閽€夋嫨浼氳瘽锛屾垨鍒涘缓鏂扮殑浼氳瘽寮€濮嬪璇?
                          </p>
                          <button
                            onClick={toggleSidebar}
                            className="px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
                          >
                            灞曞紑浼氳瘽鍒楄〃
                          </button>
                        </div>
                      </div>
                    )}
                  </div>

                  {/* 杈撳叆鍖哄煙 */}
                  {currentSession && activePanel === 'chat' && (
                    <div className="border-t border-border">
                      <TaskWorkbar
                        tasks={mergedCurrentTurnTasks}
                        historyTasks={workbarHistoryTasks}
                        currentTurnId={activeConversationTurnId}
                        isLoading={workbarLoading}
                        historyLoading={workbarHistoryLoading}
                        error={workbarError}
                        historyError={workbarHistoryError}
                        actionLoadingTaskId={workbarActionLoadingTaskId}
                        onRefresh={() => {
                          void refreshWorkbarTasks();
                        }}
                        onOpenHistory={() => {
                          void loadHistoryWorkbarTasks(currentSession.id);
                        }}
                        onCompleteTask={(task) => {
                          void handleWorkbarCompleteTask(task);
                        }}
                        onDeleteTask={(task) => {
                          void handleWorkbarDeleteTask(task);
                        }}
                        onEditTask={(task) => {
                          void handleWorkbarEditTask(task);
                        }}
                      />
                      {activeTaskReviewPanel ? (
                        <TaskDraftPanel
                          panel={activeTaskReviewPanel}
                          onConfirm={handleTaskReviewConfirm}
                          onCancel={handleTaskReviewCancel}
                        />
                      ) : null}
                      <InputArea
                        onSend={handleMessageSend}
                        onStop={abortCurrentConversation}
                        disabled={chatIsLoading || chatIsStreaming}
                        isStreaming={chatIsStreaming}
                        placeholder="输入消息..."
                        allowAttachments={true}
                        supportedFileTypes={supportedFileTypes}
                        reasoningSupported={supportsReasoning}
                        reasoningEnabled={chatConfig?.reasoningEnabled === true}
                        onReasoningToggle={(enabled) => updateChatConfig({ reasoningEnabled: enabled })}
                        showModelSelector={true}
                        selectedModelId={selectedModelId}
                        availableModels={aiModelConfigs}
                        onModelChange={setSelectedModel}
                        selectedAgentId={selectedAgentId}
                        availableAgents={agents}
                        onAgentChange={setSelectedAgent}
                        availableProjects={projects}
                        currentProject={currentProject}
                      />
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
        
        {/* MCP绠＄悊鍣?*/}
        {showMcpManager && (
          <McpManager onClose={() => setShowMcpManager(false)} />
        )}

        {/* 鏅鸿兘浣撶鐞嗗櫒 */}
        {showAgentManager && (
          <AgentManager onClose={() => setShowAgentManager(false)} />
        )}
        
        {/* AI妯″瀷绠＄悊鍣?*/}
        {showAiModelManager && (
          <AiModelManager onClose={() => setShowAiModelManager(false)} />
        )}
        
        {/* 绯荤粺涓婁笅鏂囩紪杈戝櫒 */}

        {showUserSettings && (
          <UserSettingsPanel onClose={() => setShowUserSettings(false)} />
        )}

        {/* 搴旂敤鍒楄〃锛堝脊绐楋級 */}
        <ApplicationsPanel
          isOpen={showApplicationsPanel}
          onClose={() => setShowApplicationsPanel(false)}
          title="应用列表"
          layout="modal"
        />

        {/* 琛ㄦ儏鍔╂墜宸茬Щ闄?*/}
    </div>
  );
};

export default ChatInterface;
