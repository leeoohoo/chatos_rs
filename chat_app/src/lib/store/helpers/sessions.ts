import type { Session } from '../../../types';
import type ApiClient from '../../api/client';

export const normalizeSession = (raw: any): Session => {
  const status = typeof raw?.status === 'string'
    ? raw.status.toLowerCase()
    : (raw?.archived ? 'archived' : 'active');
  const archived = raw?.archived === true || status === 'archiving' || status === 'archived';

  return {
    id: raw?.id,
    title: raw?.title ?? '',
    createdAt: new Date(raw?.created_at ?? raw?.createdAt ?? Date.now()),
    updatedAt: new Date(raw?.updated_at ?? raw?.updatedAt ?? raw?.created_at ?? raw?.createdAt ?? Date.now()),
    messageCount: raw?.messageCount ?? raw?.message_count ?? 0,
    tokenUsage: raw?.tokenUsage ?? raw?.token_usage ?? 0,
    pinned: raw?.pinned ?? false,
    archived,
    status,
    tags: raw?.tags ?? null,
    metadata: raw?.metadata ?? null,
  };
};

export const fetchSession = async (client: ApiClient, sessionId: string): Promise<Session | null> => {
  try {
    const session = await client.getSession(sessionId);
    if (!session) return null;
    return normalizeSession(session);
  } catch (error) {
    console.warn('Failed to fetch session:', error);
    return null;
  }
};
