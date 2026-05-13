import type { SessionSummaryWorkbarItem, TaskWorkbarItem } from '../TaskWorkbar';

const asRecord = (value: unknown): Record<string, unknown> => (
  value && typeof value === 'object' ? value as Record<string, unknown> : {}
);

export const normalizeWorkbarTask = (raw: unknown): TaskWorkbarItem => {
  const record = asRecord(raw);
  const statusRaw = String(record.status || 'todo').toLowerCase();
  const status: TaskWorkbarItem['status'] =
    statusRaw === 'doing' || statusRaw === 'blocked' || statusRaw === 'done'
      ? statusRaw
      : 'todo';

  const priorityRaw = String(record.priority || 'medium').toLowerCase();
  const priority: TaskWorkbarItem['priority'] =
    priorityRaw === 'high' || priorityRaw === 'low' ? priorityRaw : 'medium';

  const conversationTurnId = String(record.conversation_turn_id ?? record.conversationTurnId ?? '').trim();
  const createdAt = String(record.created_at ?? record.createdAt ?? '');
  const dueAtRaw = record.due_at ?? record.dueAt;
  const outcomeItemsCandidate = record.outcome_items ?? record.outcomeItems;
  const outcomeItemsRaw: unknown[] = Array.isArray(outcomeItemsCandidate)
    ? outcomeItemsCandidate
    : [];
  const blockerNeedsCandidate = record.blocker_needs ?? record.blockerNeeds;
  const blockerNeedsRaw: unknown[] = Array.isArray(blockerNeedsCandidate)
    ? blockerNeedsCandidate
    : [];

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
    outcomeSummary: String(record.outcome_summary ?? record.outcomeSummary ?? ''),
    outcomeItems: outcomeItemsRaw
      .map((item: unknown) => asRecord(item))
      .filter((item): item is Record<string, unknown> => item !== null)
      .map((item: Record<string, unknown>) => ({
        kind: String(item.kind || 'finding').trim() || 'finding',
        text: String(item.text || '').trim(),
        importance: (() => {
          const rawImportance = String(item.importance || '').trim().toLowerCase();
          if (rawImportance === 'high' || rawImportance === 'medium' || rawImportance === 'low') {
            return rawImportance as 'high' | 'medium' | 'low';
          }
          return undefined;
        })(),
        refs: Array.isArray(item.refs)
          ? item.refs.map((ref: unknown) => String(ref).trim()).filter((ref: string) => ref.length > 0)
          : [],
      }))
      .filter((item: { text: string }) => item.text.length > 0),
    resumeHint: String(record.resume_hint ?? record.resumeHint ?? ''),
    blockerReason: String(record.blocker_reason ?? record.blockerReason ?? ''),
    blockerNeeds: blockerNeedsRaw
      .map((item: unknown) => String(item).trim())
      .filter((item: string) => item.length > 0),
    blockerKind: String(record.blocker_kind ?? record.blockerKind ?? ''),
    completedAt: record.completed_at ?? record.completedAt ? String(record.completed_at ?? record.completedAt) : null,
    lastOutcomeAt: record.last_outcome_at ?? record.lastOutcomeAt ? String(record.last_outcome_at ?? record.lastOutcomeAt) : null,
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

  const earliestTaskWithTurn = tasks.find((task) => task.conversationTurnId.trim().length > 0);
  if (!earliestTaskWithTurn) {
    return tasks.slice(0, 8);
  }

  const currentTurnId = earliestTaskWithTurn.conversationTurnId.trim();
  return tasks.filter((task) => task.conversationTurnId.trim() === currentTurnId);
};
