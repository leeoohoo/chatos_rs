import type { ContactRecord } from '../types';

const normalizeDate = (value: unknown): Date => {
  if (typeof value === 'string' || typeof value === 'number' || value instanceof Date) {
    const parsed = new Date(value);
    if (!Number.isNaN(parsed.getTime())) {
      return parsed;
    }
  }
  return new Date();
};

export const normalizeContact = (raw: any): ContactRecord | null => {
  if (!raw || typeof raw !== 'object') {
    return null;
  }
  const id = typeof raw.id === 'string' ? raw.id.trim() : '';
  const agentId = typeof raw.agent_id === 'string' ? raw.agent_id.trim() : '';
  if (!id || !agentId) {
    return null;
  }
  const status = typeof raw.status === 'string'
    ? raw.status.toLowerCase().trim()
    : 'active';
  const name = typeof raw.agent_name_snapshot === 'string' && raw.agent_name_snapshot.trim().length > 0
    ? raw.agent_name_snapshot.trim()
    : '联系人';
  const createdAt = normalizeDate(raw.created_at);
  const updatedAt = normalizeDate(raw.updated_at);
  return {
    id,
    agentId,
    name,
    status,
    createdAt,
    updatedAt,
  };
};
