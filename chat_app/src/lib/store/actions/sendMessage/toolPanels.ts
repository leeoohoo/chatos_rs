import type {
  TaskReviewDraft,
  TaskReviewPanelState,
  UiPromptChoice,
  UiPromptField,
  UiPromptKind,
  UiPromptPanelState,
} from '../../types';
import { createInternalId } from './internalId';

const TASK_CREATE_REVIEW_REQUIRED_EVENT = 'task_create_review_required';
const UI_PROMPT_REQUIRED_EVENT = 'ui_prompt_required';

const normalizeTaskPriority = (value: unknown): TaskReviewDraft['priority'] => {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'high') return 'high';
  if (normalized === 'low') return 'low';
  return 'medium';
};

const normalizeTaskStatus = (value: unknown): TaskReviewDraft['status'] => {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'doing') return 'doing';
  if (normalized === 'blocked') return 'blocked';
  if (normalized === 'done') return 'done';
  return 'todo';
};

const parseTaskTags = (value: unknown): string[] => {
  const source = Array.isArray(value)
    ? value
    : typeof value === 'string'
      ? value.split(',')
      : [];

  const seen = new Set<string>();
  const tags: string[] = [];
  source.forEach((item) => {
    const tag = String(item ?? '').trim();
    if (!tag || seen.has(tag)) {
      return;
    }
    seen.add(tag);
    tags.push(tag);
  });
  return tags;
};

const toTaskReviewDraft = (raw: any, index: number): TaskReviewDraft => {
  const title = String(raw?.title ?? '').trim();
  const details = String(raw?.details ?? raw?.description ?? '').trim();
  const dueRaw = raw?.due_at ?? raw?.dueAt;
  const dueAt = typeof dueRaw === 'string' ? dueRaw.trim() : '';

  return {
    id: typeof raw?.id === 'string' && raw.id.trim() ? raw.id : createInternalId('draft' + (index + 1)),
    title,
    details,
    priority: normalizeTaskPriority(raw?.priority),
    status: normalizeTaskStatus(raw?.status),
    tags: parseTaskTags(raw?.tags),
    dueAt: dueAt || null,
  };
};

const normalizeUiPromptKind = (value: unknown): UiPromptKind => {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'choice') return 'choice';
  if (normalized === 'mixed') return 'mixed';
  return 'kv';
};

const normalizeUiPromptFields = (value: unknown): UiPromptField[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map((item) => {
      const key = String(item?.key ?? '').trim();
      if (!key) {
        return null;
      }
      return {
        key,
        label: typeof item?.label === 'string' ? item.label : '',
        description: typeof item?.description === 'string' ? item.description : '',
        placeholder: typeof item?.placeholder === 'string' ? item.placeholder : '',
        default: typeof item?.default === 'string' ? item.default : '',
        required: item?.required === true,
        multiline: item?.multiline === true,
        secret: item?.secret === true,
      } satisfies UiPromptField;
    })
    .filter(Boolean) as UiPromptField[];
};

const normalizeUiPromptChoice = (value: unknown): UiPromptChoice | undefined => {
  if (!value || typeof value !== 'object') {
    return undefined;
  }

  const optionsRaw = Array.isArray((value as any).options) ? (value as any).options : [];
  const options = optionsRaw
    .map((item: any) => {
      const optionValue = String(item?.value ?? '').trim();
      if (!optionValue) {
        return null;
      }
      return {
        value: optionValue,
        label: typeof item?.label === 'string' ? item.label : '',
        description: typeof item?.description === 'string' ? item.description : '',
      };
    })
    .filter(Boolean) as UiPromptChoice['options'];

  if (options.length === 0) {
    return undefined;
  }

  const multiple = (value as any).multiple === true;
  const minRaw = Number((value as any).min_selections ?? (multiple ? 0 : 0));
  const maxRaw = Number((value as any).max_selections ?? (multiple ? options.length : 1));
  const minSelections = Number.isFinite(minRaw) ? Math.max(0, Math.floor(minRaw)) : 0;
  const maxSelections = Number.isFinite(maxRaw)
    ? Math.max(0, Math.floor(maxRaw))
    : (multiple ? options.length : 1);

  return {
    multiple,
    options,
    default: (value as any).default,
    min_selections: Math.min(minSelections, maxSelections),
    max_selections: maxSelections,
  };
};

export const extractTaskReviewPanelFromToolStream = (
  streamPayload: any,
  fallbackSessionId: string,
  fallbackTurnId: string
): TaskReviewPanelState | null => {
  const rawContent = typeof streamPayload?.content === 'string' ? streamPayload.content.trim() : '';
  if (!rawContent) {
    return null;
  }

  let parsedChunk: any = null;
  try {
    parsedChunk = JSON.parse(rawContent);
  } catch (_) {
    return null;
  }

  if (parsedChunk?.event !== TASK_CREATE_REVIEW_REQUIRED_EVENT) {
    return null;
  }

  const payload = parsedChunk?.data ?? {};
  const reviewId = typeof payload?.review_id === 'string' ? payload.review_id.trim() : '';
  if (!reviewId) {
    return null;
  }

  const payloadSessionId = typeof payload?.session_id === 'string' ? payload.session_id.trim() : '';
  const sessionId = payloadSessionId || fallbackSessionId;

  const payloadTurnId = typeof payload?.conversation_turn_id === 'string'
    ? payload.conversation_turn_id.trim()
    : '';
  const conversationTurnId = payloadTurnId || fallbackTurnId;

  const rawDraftTasks = Array.isArray(payload?.draft_tasks) ? payload.draft_tasks : [];
  const drafts = rawDraftTasks.map((task: any, index: number) => toTaskReviewDraft(task, index));

  return {
    reviewId,
    sessionId,
    conversationTurnId,
    drafts,
    timeoutMs: typeof payload?.timeout_ms === 'number' ? payload.timeout_ms : undefined,
    submitting: false,
    error: null,
  };
};

export const extractUiPromptPanelFromToolStream = (
  streamPayload: any,
  fallbackSessionId: string,
  fallbackTurnId: string
): UiPromptPanelState | null => {
  const rawContent = typeof streamPayload?.content === 'string' ? streamPayload.content.trim() : '';
  if (!rawContent) {
    return null;
  }

  let parsedChunk: any = null;
  try {
    parsedChunk = JSON.parse(rawContent);
  } catch (_) {
    return null;
  }

  if (parsedChunk?.event !== UI_PROMPT_REQUIRED_EVENT) {
    return null;
  }

  const payload = parsedChunk?.data ?? {};
  const promptId = typeof payload?.prompt_id === 'string' ? payload.prompt_id.trim() : '';
  if (!promptId) {
    return null;
  }

  const payloadSessionId = typeof payload?.session_id === 'string' ? payload.session_id.trim() : '';
  const sessionId = payloadSessionId || fallbackSessionId;
  const payloadTurnId = typeof payload?.conversation_turn_id === 'string'
    ? payload.conversation_turn_id.trim()
    : '';
  const conversationTurnId = payloadTurnId || fallbackTurnId;
  const kind = normalizeUiPromptKind(payload?.kind);
  const shape = payload?.payload && typeof payload.payload === 'object' ? payload.payload : {};
  const fields = normalizeUiPromptFields((shape as any).fields);
  const choice = normalizeUiPromptChoice((shape as any).choice);

  return {
    promptId,
    sessionId,
    conversationTurnId,
    toolCallId: typeof streamPayload?.tool_call_id === 'string'
      ? streamPayload.tool_call_id
      : (typeof streamPayload?.toolCallId === 'string' ? streamPayload.toolCallId : null),
    kind,
    title: typeof payload?.title === 'string' ? payload.title : '',
    message: typeof payload?.message === 'string' ? payload.message : '',
    allowCancel: payload?.allow_cancel !== false,
    timeoutMs: typeof payload?.timeout_ms === 'number' ? payload.timeout_ms : undefined,
    payload: {
      fields,
      choice,
    },
    submitting: false,
    error: null,
  };
};
