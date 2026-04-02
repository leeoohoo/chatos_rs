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
    authorizedBuiltinMcpIds: Array.isArray(raw.authorized_builtin_mcp_ids)
      ? raw.authorized_builtin_mcp_ids
        .filter((item: unknown): item is string => typeof item === 'string')
        .map((item: string) => item.trim())
        .filter((item: string, index: number, list: string[]) => (
          item.length > 0 && list.indexOf(item) === index
        ))
      : [],
    status,
    createdAt,
    updatedAt,
  };
};
