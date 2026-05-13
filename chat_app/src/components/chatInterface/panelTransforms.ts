import type {
  TaskReviewDraft,
  TaskReviewPanelState,
  UiPromptChoice,
  UiPromptPanelState,
} from '../../lib/store/types';
import type {
  RealtimeTaskBoardPayloadWrapper,
  RealtimeUiPromptPayloadWrapper,
} from '../../lib/realtime/types';
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
  conversation_id?: string;
  conversation_turn_id?: string;
  tool_call_id?: string;
  allow_cancel?: boolean;
  timeout_ms?: number;
  created_at?: string;
  updated_at?: string;
}

interface TaskReviewRecordLike {
  review_id?: string;
  conversation_id?: string;
  conversation_turn_id?: string;
  draft_tasks?: unknown[];
  timeout_ms?: number;
}

const asRecord = (value: unknown): Record<string, unknown> => (
  value && typeof value === 'object' ? value as Record<string, unknown> : {}
);

const readString = (value: unknown): string => (
  typeof value === 'string' ? value : ''
);

const readTrimmedString = (value: unknown): string => readString(value).trim();

const normalizePromptKind = (value: unknown): UiPromptPanelState['kind'] => {
  const normalized = String(value || 'kv').trim().toLowerCase();
  if (normalized === 'choice') {
    return 'choice';
  }
  if (normalized === 'mixed') {
    return 'mixed';
  }
  return 'kv';
};

const normalizePromptPayload = (value: unknown): UiPromptPanelState['payload'] => {
  const payload = asRecord(value);
  const fields = Array.isArray(payload.fields) ? payload.fields : [];
  const choice = payload.choice
    && typeof payload.choice === 'object'
    && Array.isArray((payload.choice as UiPromptChoice).options)
    ? payload.choice as UiPromptChoice
    : undefined;
  return { fields, choice };
};

const toTaskReviewDraft = (raw: unknown, index: number): TaskReviewDraft => {
  const source = asRecord(raw);
  const title = readTrimmedString(source.title);
  const details = typeof (source.details ?? source.description) === 'string'
    ? String(source.details ?? source.description).trim()
    : '';
  const dueAt = typeof (source.due_at ?? source.dueAt) === 'string'
    ? String(source.due_at ?? source.dueAt).trim()
    : '';

  return {
    id: readTrimmedString(source.id) || `draft_${index + 1}`,
    title,
    details,
    priority: (() => {
      const normalized = String(source.priority ?? '').trim().toLowerCase();
      if (normalized === 'high') return 'high';
      if (normalized === 'low') return 'low';
      return 'medium';
    })(),
    status: (() => {
      const normalized = String(source.status ?? '').trim().toLowerCase();
      if (normalized === 'doing') return 'doing';
      if (normalized === 'blocked') return 'blocked';
      if (normalized === 'done') return 'done';
      return 'todo';
    })(),
    tags: Array.isArray(source.tags)
      ? source.tags
          .map((tag) => String(tag).trim())
          .filter((tag: string) => tag.length > 0)
      : [],
    dueAt: dueAt || null,
  };
};

const buildTaskReviewPanel = ({
  reviewId,
  sessionId,
  conversationTurnId,
  draftTasks,
  timeoutMs,
}: {
  reviewId: string;
  sessionId: string;
  conversationTurnId: string;
  draftTasks: unknown[];
  timeoutMs?: number;
}): TaskReviewPanelState | null => {
  if (!reviewId || !sessionId || !conversationTurnId) {
    return null;
  }

  return {
    reviewId,
    sessionId,
    conversationTurnId,
    drafts: draftTasks.map((task, index) => toTaskReviewDraft(task, index)),
    timeoutMs,
    submitting: false,
    error: null,
  };
};

const buildUiPromptPanel = ({
  promptId,
  sessionId,
  conversationTurnId,
  toolCallId,
  kind,
  title,
  message,
  allowCancel,
  timeoutMs,
  payload,
}: {
  promptId: string;
  sessionId: string;
  conversationTurnId: string;
  toolCallId: string | null;
  kind: UiPromptPanelState['kind'];
  title: string;
  message: string;
  allowCancel: boolean;
  timeoutMs?: number;
  payload: UiPromptPanelState['payload'];
}): UiPromptPanelState | null => {
  if (!promptId || !sessionId || !conversationTurnId) {
    return null;
  }

  return {
    promptId,
    sessionId,
    conversationTurnId,
    toolCallId,
    kind,
    title,
    message,
    allowCancel,
    timeoutMs,
    payload,
    submitting: false,
    error: null,
  };
};

export const toUiPromptPanelFromRecord = (record: unknown): UiPromptPanelState | null => {
  const normalizedRecord = asRecord(record) as UiPromptRecordLike;
  const source = normalizedRecord.prompt && typeof normalizedRecord.prompt === 'object'
    ? normalizedRecord.prompt
    : normalizedRecord;

  return buildUiPromptPanel({
    promptId: readTrimmedString(source.prompt_id),
    sessionId: readTrimmedString(source.conversation_id),
    conversationTurnId: readTrimmedString(source.conversation_turn_id),
    toolCallId: readString(source.tool_call_id) || null,
    kind: normalizePromptKind(source.kind),
    title: readString(source.title),
    message: readString(source.message),
    allowCancel: source.allow_cancel !== false,
    timeoutMs: typeof source.timeout_ms === 'number' ? source.timeout_ms : undefined,
    payload: normalizePromptPayload(source.payload),
  });
};

export const toTaskReviewPanelFromRealtimePayload = (
  payload: RealtimeTaskBoardPayloadWrapper,
): TaskReviewPanelState | null => buildTaskReviewPanel({
  reviewId: readTrimmedString(payload.review_id),
  sessionId: readTrimmedString(payload.conversation_id),
  conversationTurnId: readTrimmedString(payload.conversation_turn_id),
  draftTasks: Array.isArray(payload.draft_tasks) ? payload.draft_tasks : [],
  timeoutMs: typeof payload.timeout_ms === 'number' ? payload.timeout_ms : undefined,
});

export const toTaskReviewPanelFromRecord = (
  record: unknown,
): TaskReviewPanelState | null => {
  const source = asRecord(record) as TaskReviewRecordLike;
  return buildTaskReviewPanel({
    reviewId: readTrimmedString(source.review_id),
    sessionId: readTrimmedString(source.conversation_id),
    conversationTurnId: readTrimmedString(source.conversation_turn_id),
    draftTasks: Array.isArray(source.draft_tasks) ? source.draft_tasks : [],
    timeoutMs: typeof source.timeout_ms === 'number' ? source.timeout_ms : undefined,
  });
};

export const toUiPromptPanelFromRealtimePayload = (
  payload: RealtimeUiPromptPayloadWrapper,
): UiPromptPanelState | null => buildUiPromptPanel({
  promptId: readTrimmedString(payload.prompt_id),
  sessionId: readTrimmedString(payload.conversation_id),
  conversationTurnId: readTrimmedString(payload.conversation_turn_id),
  toolCallId: readString(payload.tool_call_id) || null,
  kind: normalizePromptKind(payload.prompt_kind),
  title: readString(payload.title),
  message: readString(payload.message),
  allowCancel: payload.allow_cancel !== false,
  timeoutMs: typeof payload.timeout_ms === 'number' ? payload.timeout_ms : undefined,
  payload: normalizePromptPayload(payload.payload),
});

export const normalizeUiPromptHistoryItem = (raw: unknown): UiPromptHistoryItem | null => {
  if (!raw || typeof raw !== 'object') {
    return null;
  }

  const record = raw as UiPromptRecordLike;
  const promptId = readTrimmedString(record.id);
  const sessionId = readTrimmedString(record.conversation_id);
  if (!promptId || !sessionId) {
    return null;
  }

  const prompt = asRecord(record.prompt);
  const response = record.response && typeof record.response === 'object' ? record.response : null;

  return {
    id: promptId,
    sessionId,
    conversationTurnId: readTrimmedString(record.conversation_turn_id),
    kind: String(record.kind || ''),
    status: String(record.status || ''),
    title: readString(prompt.title),
    message: readString(prompt.message),
    prompt,
    response,
    createdAt: String(record.created_at || ''),
    updatedAt: String(record.updated_at || ''),
  };
};
