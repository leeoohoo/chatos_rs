// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { SessionMemorySummary, ContactAgentRecall } from './useContactMemoryContext';

export interface MemoryCacheEntry {
  sessionMemorySummaries: SessionMemorySummary[];
  agentRecalls: ContactAgentRecall[];
}

const toTimestamp = (value: string | null | undefined): number => {
  const parsed = value ? new Date(value).getTime() : Number.NaN;
  return Number.isFinite(parsed) ? parsed : 0;
};

const asRecord = (value: unknown): Record<string, unknown> | null => {
  if (!value || typeof value !== 'object') {
    return null;
  }
  return value as Record<string, unknown>;
};

const readString = (record: Record<string, unknown> | null, key: string): string => {
  if (!record) {
    return '';
  }
  const value = record[key];
  return typeof value === 'string' ? value : '';
};

export const normalizeAgentRecalls = (rows: unknown[]): ContactAgentRecall[] => {
  const normalized = rows
    .map((item) => {
      const record = asRecord(item);
      return {
        id: String(record?.id || ''),
        recallKey: String(record?.recall_key || ''),
        recallText: String(record?.recall_text || ''),
        level: Number.isFinite(Number(record?.level)) ? Number(record?.level) : 0,
        confidence: typeof record?.confidence === 'number' ? record.confidence : null,
        lastSeenAt: readString(record, 'last_seen_at') || null,
        updatedAt: String(record?.updated_at || ''),
      };
    })
    .filter((item) => item.id && item.recallKey);

  return normalized
    .sort((left, right) => {
      if (right.level !== left.level) {
        return right.level - left.level;
      }
      return toTimestamp(right.updatedAt) - toTimestamp(left.updatedAt);
    })
    .slice(0, 1);
};

export const buildMemoryLoadKey = (
  sessionId: string,
  currentContactId: string,
  currentProjectIdForMemory: string,
): string => {
  const normalizedContactId = currentContactId.trim();
  const normalizedProjectId = currentProjectIdForMemory.trim();
  return `${sessionId}::${normalizedContactId || '-'}::${normalizedProjectId || '-'}`;
};
