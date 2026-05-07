import type {
  TaskReviewDraft,
  TaskReviewPanelState,
  UiPromptPanelState,
} from '../store/types';
import { createInternalId } from './internalIds';
import { asRecord } from './normalizerUtils';
import {
  normalizeTaskPriority,
  normalizeTaskStatus,
  normalizeUiPromptChoice,
  normalizeUiPromptFields,
  normalizeUiPromptKind,
  parseTaskTags,
} from './uiPrompts';

const TASK_CREATE_REVIEW_REQUIRED_EVENT = 'task_create_review_required';
const TASK_BOARD_UPDATED_EVENT = 'task_board_updated';
const UI_PROMPT_REQUIRED_EVENT = 'ui_prompt_required';

const readTrimmedString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const readRawContent = (streamPayload: unknown): string => {
  const source = asRecord(streamPayload);
  return typeof source?.content === 'string' ? source.content.trim() : '';
};

const parseEventEnvelope = (streamPayload: unknown): {
  source: Record<string, unknown>;
  envelope: Record<string, unknown>;
  payload: Record<string, unknown>;
} | null => {
  const source = asRecord(streamPayload) || {};
  const rawContent = readRawContent(streamPayload);
  if (!rawContent) {
    return null;
  }

  let parsedChunk: unknown = null;
  try {
    parsedChunk = JSON.parse(rawContent);
  } catch {
    return null;
  }

  const envelope = asRecord(parsedChunk);
  if (!envelope) {
    return null;
  }

  return {
    source,
    envelope,
    payload: asRecord(envelope.data) || {},
  };
};

const toTaskReviewDraft = (raw: unknown, index: number): TaskReviewDraft => {
  const source = asRecord(raw) || {};
  const title = readTrimmedString(source.title);
  const details = readTrimmedString(source.details ?? source.description);
  const dueAt = readTrimmedString(source.due_at ?? source.dueAt);

  return {
    id: readTrimmedString(source.id) || createInternalId(`draft${index + 1}`),
    title,
    details,
    priority: normalizeTaskPriority(source.priority),
    status: normalizeTaskStatus(source.status),
    tags: parseTaskTags(source.tags),
    dueAt: dueAt || null,
  };
};

export const extractTaskReviewPanelFromToolStream = (
  streamPayload: unknown,
  fallbackSessionId: string,
  fallbackTurnId: string,
): TaskReviewPanelState | null => {
  const parsed = parseEventEnvelope(streamPayload);
  if (!parsed || parsed.envelope.event !== TASK_CREATE_REVIEW_REQUIRED_EVENT) {
    return null;
  }

  const reviewId = readTrimmedString(parsed.payload.review_id);
  if (!reviewId) {
    return null;
  }

  const sessionId = readTrimmedString(parsed.payload.conversation_id) || fallbackSessionId;
  const conversationTurnId = readTrimmedString(parsed.payload.conversation_turn_id) || fallbackTurnId;
  const rawDraftTasks = Array.isArray(parsed.payload.draft_tasks) ? parsed.payload.draft_tasks : [];
  const drafts = rawDraftTasks.map((task, index) => toTaskReviewDraft(task, index));

  return {
    reviewId,
    sessionId,
    conversationTurnId,
    drafts,
    timeoutMs: typeof parsed.payload.timeout_ms === 'number' ? parsed.payload.timeout_ms : undefined,
    submitting: false,
    error: null,
  };
};

export const extractUiPromptPanelFromToolStream = (
  streamPayload: unknown,
  fallbackSessionId: string,
  fallbackTurnId: string,
): UiPromptPanelState | null => {
  const parsed = parseEventEnvelope(streamPayload);
  if (!parsed || parsed.envelope.event !== UI_PROMPT_REQUIRED_EVENT) {
    return null;
  }

  const promptId = readTrimmedString(parsed.payload.prompt_id);
  if (!promptId) {
    return null;
  }

  const sessionId = readTrimmedString(parsed.payload.conversation_id) || fallbackSessionId;
  const conversationTurnId = readTrimmedString(parsed.payload.conversation_turn_id) || fallbackTurnId;
  const shape = asRecord(parsed.payload.payload) || {};

  return {
    promptId,
    sessionId,
    conversationTurnId,
    toolCallId: readTrimmedString(parsed.source.tool_call_id)
      || readTrimmedString(parsed.source.toolCallId)
      || null,
    kind: normalizeUiPromptKind(parsed.payload.kind),
    title: typeof parsed.payload.title === 'string' ? parsed.payload.title : '',
    message: typeof parsed.payload.message === 'string' ? parsed.payload.message : '',
    allowCancel: parsed.payload.allow_cancel !== false,
    timeoutMs: typeof parsed.payload.timeout_ms === 'number' ? parsed.payload.timeout_ms : undefined,
    payload: {
      fields: normalizeUiPromptFields(shape.fields),
      choice: normalizeUiPromptChoice(shape.choice),
    },
    submitting: false,
    error: null,
  };
};

export const extractTaskBoardUpdatedEvent = (
  streamPayload: unknown,
): {
  sessionId: string;
  conversationTurnId: string | null;
  taskBoard: string;
} | null => {
  const parsed = parseEventEnvelope(streamPayload);
  if (!parsed || parsed.envelope.event !== TASK_BOARD_UPDATED_EVENT) {
    return null;
  }

  const sessionId = readTrimmedString(parsed.payload.conversation_id);
  const taskBoard = readTrimmedString(parsed.payload.task_board);
  if (!sessionId || !taskBoard) {
    return null;
  }

  return {
    sessionId,
    conversationTurnId: readTrimmedString(parsed.payload.conversation_turn_id) || null,
    taskBoard,
  };
};
