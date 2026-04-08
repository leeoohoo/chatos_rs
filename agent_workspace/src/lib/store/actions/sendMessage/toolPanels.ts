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

type AnyRecord = Record<string, unknown>;

const asRecord = (value: unknown): AnyRecord | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as AnyRecord
    : null
);

const normalizeTaskPriority = (value: unknown): TaskReviewDraft['priority'] => {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'high') return 'high';
  if (normalized === 'low') return 'low';
  return 'medium';
};

const normalizeTaskStatus = (value: unknown): TaskReviewDraft['status'] => {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'pending_execute') return 'pending_execute';
  if (normalized === 'running') return 'running';
  if (normalized === 'paused') return 'paused';
  if (normalized === 'blocked') return 'blocked';
  if (normalized === 'completed') return 'completed';
  if (normalized === 'failed') return 'failed';
  if (normalized === 'cancelled') return 'cancelled';
  if (normalized === 'skipped') return 'skipped';
  return 'pending_confirm';
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

const parseStringArray = (value: unknown): string[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  const out: string[] = [];
  value.forEach((item) => {
    const normalized = String(item ?? '').trim();
    if (!normalized || out.includes(normalized)) {
      return;
    }
    out.push(normalized);
  });
  return out;
};

const parseTaskContextAssets = (value: unknown): NonNullable<TaskReviewDraft['plannedContextAssets']> => {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map((item) => {
      const source = asRecord(item) || {};
      const assetType = String(source.asset_type ?? source.assetType ?? '').trim();
      const assetId = String(source.asset_id ?? source.assetId ?? '').trim();
      if (!assetType || !assetId) {
        return null;
      }
      return {
        assetType,
        assetId,
        displayName: typeof (source.display_name ?? source.displayName) === 'string'
          ? String(source.display_name ?? source.displayName).trim()
          : null,
        sourceType: typeof (source.source_type ?? source.sourceType) === 'string'
          ? String(source.source_type ?? source.sourceType).trim()
          : null,
        sourcePath: typeof (source.source_path ?? source.sourcePath) === 'string'
          ? String(source.source_path ?? source.sourcePath).trim()
          : null,
      };
    })
    .filter(Boolean) as NonNullable<TaskReviewDraft['plannedContextAssets']>;
};

const parseExecutionResultContract = (value: unknown): TaskReviewDraft['executionResultContract'] => {
  const source = asRecord(value);
  if (!source) {
    return null;
  }
  return {
    resultRequired: source.result_required !== false && source.resultRequired !== false,
    preferredFormat: typeof (source.preferred_format ?? source.preferredFormat) === 'string'
      ? String(source.preferred_format ?? source.preferredFormat).trim()
      : null,
  };
};

const toTaskReviewDraft = (raw: unknown, index: number): TaskReviewDraft => {
  const source = asRecord(raw) || {};
  const title = String(source.title ?? '').trim();
  const details = String(source.details ?? source.description ?? '').trim();
  const dueRaw = source.due_at ?? source.dueAt;
  const dueAt = typeof dueRaw === 'string' ? dueRaw.trim() : '';

  return {
    id: typeof source.id === 'string' && source.id.trim()
      ? source.id
      : createInternalId(`draft${index + 1}`),
    title,
    details,
    priority: normalizeTaskPriority(source.priority),
    status: normalizeTaskStatus(source.status),
    tags: parseTaskTags(source.tags),
    dueAt: dueAt || null,
    taskRef: typeof (source.task_ref ?? source.taskRef) === 'string'
      ? String(source.task_ref ?? source.taskRef).trim()
      : null,
    taskKind: typeof (source.task_kind ?? source.taskKind) === 'string'
      ? String(source.task_kind ?? source.taskKind).trim()
      : null,
    dependsOnRefs: parseStringArray(
      source.depends_on_refs ?? source.dependsOnRefs,
    ),
    verificationOfRefs: parseStringArray(
      source.verification_of_refs ?? source.verificationOfRefs,
    ),
    acceptanceCriteria: parseStringArray(
      source.acceptance_criteria ?? source.acceptanceCriteria,
    ),
    plannedBuiltinMcpIds: parseStringArray(
      source.planned_builtin_mcp_ids ?? source.plannedBuiltinMcpIds,
    ),
    plannedContextAssets: parseTaskContextAssets(
      source.planned_context_assets ?? source.plannedContextAssets,
    ),
    executionResultContract: parseExecutionResultContract(
      source.execution_result_contract ?? source.executionResultContract,
    ),
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
  const source = asRecord(value);
  if (!source) {
    return undefined;
  }

  const optionsRaw = Array.isArray(source.options) ? source.options : [];
  const options = optionsRaw
    .map((item) => {
      const option = asRecord(item) || {};
      const optionValue = String(option.value ?? '').trim();
      if (!optionValue) {
        return null;
      }
      return {
        value: optionValue,
        label: typeof option.label === 'string' ? option.label : '',
        description: typeof option.description === 'string' ? option.description : '',
      };
    })
    .filter(Boolean) as UiPromptChoice['options'];

  if (options.length === 0) {
    return undefined;
  }

  const multiple = source.multiple === true;
  const minRaw = Number(source.min_selections ?? (multiple ? 0 : 0));
  const maxRaw = Number(source.max_selections ?? (multiple ? options.length : 1));
  const minSelections = Number.isFinite(minRaw) ? Math.max(0, Math.floor(minRaw)) : 0;
  const maxSelections = Number.isFinite(maxRaw)
    ? Math.max(0, Math.floor(maxRaw))
    : (multiple ? options.length : 1);

  return {
    multiple,
    options,
    default: source.default as UiPromptChoice['default'],
    min_selections: Math.min(minSelections, maxSelections),
    max_selections: maxSelections,
  };
};

export const extractTaskReviewPanelFromToolStream = (
  streamPayload: unknown,
  fallbackSessionId: string,
  fallbackTurnId: string,
): TaskReviewPanelState | null => {
  const source = asRecord(streamPayload) || {};
  const rawContent = typeof source.content === 'string' ? source.content.trim() : '';
  if (!rawContent) {
    return null;
  }

  let parsedChunk: unknown = null;
  try {
    parsedChunk = JSON.parse(rawContent);
  } catch (_) {
    return null;
  }

  const eventEnvelope = asRecord(parsedChunk);
  if (eventEnvelope?.event !== TASK_CREATE_REVIEW_REQUIRED_EVENT) {
    return null;
  }

  const payload = asRecord(eventEnvelope?.data) || {};
  const reviewId = typeof payload.review_id === 'string' ? payload.review_id.trim() : '';
  if (!reviewId) {
    return null;
  }

  const payloadSessionId = typeof payload.session_id === 'string' ? payload.session_id.trim() : '';
  const sessionId = payloadSessionId || fallbackSessionId;

  const payloadTurnId = typeof payload.conversation_turn_id === 'string'
    ? payload.conversation_turn_id.trim()
    : '';
  const conversationTurnId = payloadTurnId || fallbackTurnId;

  const rawDraftTasks = Array.isArray(payload.draft_tasks) ? payload.draft_tasks : [];
  const drafts = rawDraftTasks.map((task, index) => toTaskReviewDraft(task, index));

  return {
    reviewId,
    sessionId,
    conversationTurnId,
    actionRequestId: null,
    source: 'legacy',
    drafts,
    timeoutMs: typeof payload.timeout_ms === 'number' ? payload.timeout_ms : undefined,
    submitting: false,
    error: null,
  };
};

export const extractUiPromptPanelFromToolStream = (
  streamPayload: unknown,
  fallbackSessionId: string,
  fallbackTurnId: string,
): UiPromptPanelState | null => {
  const source = asRecord(streamPayload) || {};
  const rawContent = typeof source.content === 'string' ? source.content.trim() : '';
  if (!rawContent) {
    return null;
  }

  let parsedChunk: unknown = null;
  try {
    parsedChunk = JSON.parse(rawContent);
  } catch (_) {
    return null;
  }

  const eventEnvelope = asRecord(parsedChunk);
  if (eventEnvelope?.event !== UI_PROMPT_REQUIRED_EVENT) {
    return null;
  }

  const payload = asRecord(eventEnvelope?.data) || {};
  const promptId = typeof payload.prompt_id === 'string' ? payload.prompt_id.trim() : '';
  if (!promptId) {
    return null;
  }

  const payloadSessionId = typeof payload.session_id === 'string' ? payload.session_id.trim() : '';
  const sessionId = payloadSessionId || fallbackSessionId;
  const payloadTurnId = typeof payload.conversation_turn_id === 'string'
    ? payload.conversation_turn_id.trim()
    : '';
  const conversationTurnId = payloadTurnId || fallbackTurnId;
  const kind = normalizeUiPromptKind(payload.kind);
  const shape = asRecord(payload.payload) || {};
  const fields = normalizeUiPromptFields(shape.fields);
  const choice = normalizeUiPromptChoice(shape.choice);

  return {
    promptId,
    sessionId,
    conversationTurnId,
    actionRequestId: null,
    source: 'legacy',
    toolCallId: typeof source.tool_call_id === 'string'
      ? source.tool_call_id
      : (typeof source.toolCallId === 'string' ? source.toolCallId : null),
    kind,
    title: typeof payload.title === 'string' ? payload.title : '',
    message: typeof payload.message === 'string' ? payload.message : '',
    allowCancel: payload.allow_cancel !== false,
    timeoutMs: typeof payload.timeout_ms === 'number' ? payload.timeout_ms : undefined,
    payload: {
      fields,
      choice,
    },
    submitting: false,
    error: null,
  };
};
