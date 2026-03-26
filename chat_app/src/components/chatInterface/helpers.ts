import type { UiPromptPanelState } from '../../lib/store/types';
import type { SessionSummaryWorkbarItem, TaskWorkbarItem } from '../TaskWorkbar';
import type { UiPromptHistoryItem } from './types';

export const toUiPromptPanelFromRecord = (record: any): UiPromptPanelState | null => {
  const source = record?.prompt && typeof record.prompt === 'object' ? record.prompt : record;
  const promptId = typeof source?.prompt_id === 'string' ? source.prompt_id.trim() : '';
  const sessionId = typeof source?.session_id === 'string' ? source.session_id.trim() : '';
  const conversationTurnId = typeof source?.conversation_turn_id === 'string'
    ? source.conversation_turn_id.trim()
    : '';
  if (!promptId || !sessionId || !conversationTurnId) {
    return null;
  }

  const kindRaw = String(source?.kind || 'kv').trim().toLowerCase();
  const kind = kindRaw === 'choice' ? 'choice' : (kindRaw === 'mixed' ? 'mixed' : 'kv');

  const payload = source?.payload && typeof source.payload === 'object' ? source.payload : {};
  const fields = Array.isArray((payload as any).fields) ? (payload as any).fields : [];
  const choice = (payload as any).choice && typeof (payload as any).choice === 'object'
    ? (payload as any).choice
    : undefined;

  return {
    promptId,
    sessionId,
    conversationTurnId,
    toolCallId: typeof source?.tool_call_id === 'string' ? source.tool_call_id : null,
    kind,
    title: typeof source?.title === 'string' ? source.title : '',
    message: typeof source?.message === 'string' ? source.message : '',
    allowCancel: source?.allow_cancel !== false,
    timeoutMs: typeof source?.timeout_ms === 'number' ? source.timeout_ms : undefined,
    payload: { fields, choice },
    submitting: false,
    error: null,
  };
};

export const normalizeUiPromptHistoryItem = (raw: any): UiPromptHistoryItem | null => {
  if (!raw || typeof raw !== 'object') {
    return null;
  }

  const promptId = typeof raw.id === 'string' ? raw.id.trim() : '';
  const sessionId = typeof raw.session_id === 'string' ? raw.session_id.trim() : '';
  const conversationTurnId = typeof raw.conversation_turn_id === 'string'
    ? raw.conversation_turn_id.trim()
    : '';
  if (!promptId || !sessionId) {
    return null;
  }

  const prompt = raw.prompt && typeof raw.prompt === 'object' ? raw.prompt : {};
  const response = raw.response && typeof raw.response === 'object' ? raw.response : null;
  const title = typeof (prompt as any).title === 'string' ? (prompt as any).title : '';
  const message = typeof (prompt as any).message === 'string' ? (prompt as any).message : '';

  return {
    id: promptId,
    sessionId,
    conversationTurnId,
    kind: String(raw.kind || ''),
    status: String(raw.status || ''),
    title,
    message,
    prompt,
    response,
    createdAt: String(raw.created_at || ''),
    updatedAt: String(raw.updated_at || ''),
  };
};

export const formatSummaryCreatedAt = (value: string): string => {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value || '-';
  }
  return parsed.toLocaleString('zh-CN', { hour12: false });
};

export const buildSupportedFileTypes = (supportsImages: boolean): string[] => (
  supportsImages
    ? ['image/*', 'text/*', 'application/json', 'application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']
    : ['text/*', 'application/json', 'application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']
);

export const resolveModelSupportFlags = (
  selectedModelId: string | null,
  aiModelConfigs: any[],
): { supportsImages: boolean; supportsReasoning: boolean } => {
  if (!selectedModelId) {
    return { supportsImages: false, supportsReasoning: false };
  }
  const matched = (aiModelConfigs || []).find((item: any) => item?.id === selectedModelId);
  return {
    supportsImages: matched?.supports_images === true,
    supportsReasoning: matched?.supports_reasoning === true,
  };
};

export const pickFirstSessionPanel = <T,>(
  panelsBySession: Record<string, T[] | undefined> | undefined,
  sessionId: string | null | undefined,
): T | null => {
  const normalizedSessionId = typeof sessionId === 'string' ? sessionId.trim() : '';
  if (!normalizedSessionId) {
    return null;
  }
  const panels = panelsBySession?.[normalizedSessionId];
  if (!Array.isArray(panels) || panels.length === 0) {
    return null;
  }
  return panels[0] || null;
};

export const pickSessionScopedState = <T,>(
  stateBySession: Record<string, T | undefined> | undefined,
  sessionId: string | null | undefined,
): T | undefined => {
  const normalizedSessionId = typeof sessionId === 'string' ? sessionId.trim() : '';
  if (!normalizedSessionId) {
    return undefined;
  }
  return stateBySession?.[normalizedSessionId];
};

export const isTaskMutationToolName = (name: unknown): boolean => {
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
};

export const collectMessageToolCalls = (message: any): any[] => {
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
};

export const shouldRefreshForTaskMutationToolCall = (toolCall: any): boolean => {
  if (isTaskMutationToolName(toolCall?.name)) {
    return true;
  }
  return false;
};

export const hasToolCallError = (toolCall: any): boolean => {
  if (toolCall?.error === null || toolCall?.error === undefined) {
    return false;
  }
  if (typeof toolCall.error === 'string') {
    return toolCall.error.trim().length > 0;
  }
  return true;
};

export const parseMaybeJsonValue = (value: unknown): unknown => {
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
};

export const collectTaskIdsFromToolResult = (
  value: unknown,
  collector: Set<string>,
  depth = 0
): void => {
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
};

export const extractTaskIdsFromToolCall = (toolCall: any): string[] => {
  const output = new Set<string>();

  const candidates = [
    toolCall?.result,
    toolCall?.finalResult,
    parseMaybeJsonValue(toolCall?.result),
    parseMaybeJsonValue(toolCall?.finalResult),
  ];

  candidates.forEach((item) => collectTaskIdsFromToolResult(item, output));
  return Array.from(output);
};

export const normalizeWorkbarTask = (raw: any): TaskWorkbarItem => {
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
};

export const normalizeWorkbarSummary = (raw: any): SessionSummaryWorkbarItem => ({
  id: String(raw?.id || '').trim(),
  summaryText: String(raw?.summary_text ?? raw?.summaryText ?? ''),
  summaryModel: String(raw?.summary_model ?? raw?.summaryModel ?? ''),
  triggerType: String(raw?.trigger_type ?? raw?.triggerType ?? ''),
  sourceMessageCount: Number(raw?.source_message_count ?? raw?.sourceMessageCount ?? 0),
  sourceEstimatedTokens: Number(raw?.source_estimated_tokens ?? raw?.sourceEstimatedTokens ?? 0),
  createdAt: String(raw?.created_at ?? raw?.createdAt ?? ''),
  status: typeof raw?.status === 'string' ? raw.status : undefined,
  errorMessage: typeof raw?.error_message === 'string'
    ? raw.error_message
    : (typeof raw?.errorMessage === 'string' ? raw.errorMessage : null),
});

export const selectLatestTurnTasks = (tasks: TaskWorkbarItem[]): TaskWorkbarItem[] => {
  if (tasks.length === 0) {
    return [];
  }

  const latestTaskWithTurn = tasks.find((task) => task.conversationTurnId.trim().length > 0);
  if (!latestTaskWithTurn) {
    return tasks.slice(0, 8);
  }

  const latestTurnId = latestTaskWithTurn.conversationTurnId.trim();
  return tasks.filter((task) => task.conversationTurnId.trim() === latestTurnId);
};
