import type { Session } from '../../../types';
import type ApiClient from '../../api/client';

export const normalizeSession = (raw: any): Session => ({
  id: raw?.id,
  title: raw?.title ?? '',
  createdAt: new Date(raw?.created_at ?? raw?.createdAt ?? Date.now()),
  updatedAt: new Date(raw?.updated_at ?? raw?.updatedAt ?? raw?.created_at ?? raw?.createdAt ?? Date.now()),
  messageCount: raw?.messageCount ?? 0,
  tokenUsage: raw?.tokenUsage ?? 0,
  pinned: raw?.pinned ?? false,
  archived: raw?.archived ?? false,
  tags: raw?.tags ?? null,
  metadata: raw?.metadata ?? null,
});

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
