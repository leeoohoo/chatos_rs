import type { AiModelConfig, Message } from '../../types';
import type { UiPromptChoice, UiPromptPanelState } from '../../lib/store/types';
import type { SessionSummaryWorkbarItem, TaskWorkbarItem } from '../TaskWorkbar';
import type { UiPromptHistoryItem } from './types';

interface UiPromptRecordLike {
  prompt?: Record<string, unknown>;
  response?: Record<string, unknown> | null;
  payload?: Record<string, unknown>;
  id?: string;
  kind?: string;
  status?: string;
  title?: string;
  message?: string;
  prompt_id?: string;
  session_id?: string;
  conversation_turn_id?: string;
  tool_call_id?: string;
  allow_cancel?: boolean;
  timeout_ms?: number;
  created_at?: string;
  updated_at?: string;
}

interface ToolCallLike {
  id?: string;
  name?: string;
  tool_call_id?: string;
  toolCallId?: string;
  result?: unknown;
  finalResult?: unknown;
  error?: unknown;
  completed?: boolean;
}

interface MessageWithToolCalls {
  sessionId?: string;
  toolCalls?: ToolCallLike[];
  metadata?: (NonNullable<Message['metadata']> & {
    conversation_turn_id?: string;
    toolCalls?: ToolCallLike[];
  }) | undefined;
}

const asRecord = (value: unknown): Record<string, unknown> => (
  value && typeof value === 'object' ? value as Record<string, unknown> : {}
);

const getString = (value: unknown): string => (typeof value === 'string' ? value : '');

export const toUiPromptPanelFromRecord = (record: unknown): UiPromptPanelState | null => {
  const normalizedRecord = asRecord(record) as UiPromptRecordLike;
  const source = normalizedRecord.prompt && typeof normalizedRecord.prompt === 'object'
    ? normalizedRecord.prompt
    : normalizedRecord;
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

  const payload = asRecord(source?.payload);
  const fields = Array.isArray(payload.fields) ? payload.fields : [];
  const choice = payload.choice && typeof payload.choice === 'object' && Array.isArray((payload.choice as UiPromptChoice).options)
    ? payload.choice as UiPromptChoice
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

export const normalizeUiPromptHistoryItem = (raw: unknown): UiPromptHistoryItem | null => {
  if (!raw || typeof raw !== 'object') {
    return null;
  }

  const record = raw as UiPromptRecordLike;
  const promptId = typeof record.id === 'string' ? record.id.trim() : '';
  const sessionId = typeof record.session_id === 'string' ? record.session_id.trim() : '';
  const conversationTurnId = typeof record.conversation_turn_id === 'string'
    ? record.conversation_turn_id.trim()
    : '';
  if (!promptId || !sessionId) {
    return null;
  }

  const prompt = asRecord(record.prompt);
  const response = record.response && typeof record.response === 'object' ? record.response : null;
  const title = getString(prompt.title);
  const message = getString(prompt.message);

  return {
    id: promptId,
    sessionId,
    conversationTurnId,
    kind: String(record.kind || ''),
    status: String(record.status || ''),
    title,
    message,
    prompt,
    response,
    createdAt: String(record.created_at || ''),
    updatedAt: String(record.updated_at || ''),
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
  aiModelConfigs: AiModelConfig[],
): { supportsImages: boolean; supportsReasoning: boolean } => {
  if (!selectedModelId) {
    return { supportsImages: false, supportsReasoning: false };
  }
  const matched = (aiModelConfigs || []).find((item) => item?.id === selectedModelId);
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

export const collectMessageToolCalls = (message: MessageWithToolCalls): ToolCallLike[] => {
  const topLevel = Array.isArray(message?.toolCalls) ? message.toolCalls : [];
  const metadataLevel = Array.isArray(message?.metadata?.toolCalls)
    ? message.metadata.toolCalls
    : [];

  const merged = [...metadataLevel, ...topLevel];
  if (merged.length <= 1) {
    return merged;
  }

  const seen = new Set<string>();
  return merged.filter((toolCall, index) => {
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

export const shouldRefreshForTaskMutationToolCall = (toolCall: ToolCallLike): boolean => {
  if (isTaskMutationToolName(toolCall?.name)) {
    return true;
  }
  return false;
};

export const hasToolCallError = (toolCall: ToolCallLike): boolean => {
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

export const extractTaskIdsFromToolCall = (toolCall: ToolCallLike): string[] => {
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

export const normalizeWorkbarTask = (raw: unknown): TaskWorkbarItem => {
  const record = asRecord(raw);
  const statusRaw = String(record.status || 'pending_confirm').toLowerCase();
  const status: TaskWorkbarItem['status'] =
    statusRaw === 'pending_confirm'
    || statusRaw === 'pending_execute'
    || statusRaw === 'running'
    || statusRaw === 'completed'
    || statusRaw === 'failed'
    || statusRaw === 'cancelled'
      ? statusRaw
      : 'pending_confirm';

  const priorityRaw = String(record.priority || 'medium').toLowerCase();
  const priority: TaskWorkbarItem['priority'] =
    priorityRaw === 'high' || priorityRaw === 'low' ? priorityRaw : 'medium';

  const conversationTurnId = String(record.conversation_turn_id ?? record.conversationTurnId ?? '').trim();
  const createdAt = String(record.created_at ?? record.createdAt ?? '');
  const dueAtRaw = record.due_at ?? record.dueAt;

  return {
    id: String(record.id || '').trim(),
    title: String(record.title || ''),
    details: String(record.details || record.description || ''),
    status,
    priority,
    conversationTurnId,
    createdAt,
    dueAt: dueAtRaw ? String(dueAtRaw) : null,
    tags: Array.isArray(record.tags)
      ? record.tags
          .map((tag) => String(tag).trim())
          .filter((tag: string) => tag.length > 0)
      : [],
  };
};

export const normalizeWorkbarSummary = (raw: unknown): SessionSummaryWorkbarItem => {
  const record = asRecord(raw);
  return {
    id: String(record.id || '').trim(),
    summaryText: String(record.summary_text ?? record.summaryText ?? ''),
    summaryModel: String(record.summary_model ?? record.summaryModel ?? ''),
    triggerType: String(record.trigger_type ?? record.triggerType ?? ''),
    sourceMessageCount: Number(record.source_message_count ?? record.sourceMessageCount ?? 0),
    sourceEstimatedTokens: Number(record.source_estimated_tokens ?? record.sourceEstimatedTokens ?? 0),
    createdAt: String(record.created_at ?? record.createdAt ?? ''),
    status: typeof record.status === 'string' ? record.status : undefined,
    errorMessage: typeof record.error_message === 'string'
      ? record.error_message
      : (typeof record.errorMessage === 'string' ? record.errorMessage : null),
  };
};

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
