import type { Session } from '../../types';
import { readSessionRuntimeFromMetadata } from '../../lib/store/helpers/sessionRuntime';

export interface ContactSessionRef {
  id: string;
  agentId: string;
}

export const normalizeProjectScopeId = (projectId: string | null | undefined): string => {
  const trimmed = typeof projectId === 'string' ? projectId.trim() : '';
  return trimmed.length > 0 ? trimmed : '0';
};

export const resolveSessionProjectScopeId = (
  session: Session | Record<string, any> | null | undefined,
): string => {
  if (!session) {
    return '0';
  }
  const rawProjectId = typeof (session as any).projectId === 'string'
    ? (session as any).projectId.trim()
    : (typeof (session as any).project_id === 'string'
      ? (session as any).project_id.trim()
      : '');
  if (rawProjectId.length > 0) {
    return normalizeProjectScopeId(rawProjectId);
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  return normalizeProjectScopeId(runtime?.projectId ?? null);
};

export const resolveSessionTimestamp = (
  session: Session | Record<string, any> | null | undefined,
): number => {
  if (!session) {
    return 0;
  }
  const raw = (session as any).updatedAt
    ?? (session as any).updated_at
    ?? (session as any).createdAt
    ?? (session as any).created_at
    ?? Date.now();
  const ts = new Date(raw).getTime();
  return Number.isFinite(ts) ? ts : 0;
};

export const isSessionActive = (
  session: Session | Record<string, any> | null | undefined,
): boolean => {
  if (!session) {
    return false;
  }
  const archived = (session as any).archived === true;
  const status = typeof (session as any).status === 'string'
    ? (session as any).status.toLowerCase()
    : '';
  return !archived && status !== 'archived' && status !== 'archiving';
};

export const resolveContactAgentIdFromSession = (
  session: Session | Record<string, any> | null | undefined,
): string | null => {
  if (!session) {
    return null;
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  if (!runtime?.contactAgentId) {
    return null;
  }
  const trimmed = runtime.contactAgentId.trim();
  return trimmed.length > 0 ? trimmed : null;
};

export const resolveContactIdFromSession = (
  session: Session | Record<string, any> | null | undefined,
): string | null => {
  if (!session) {
    return null;
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  if (!runtime?.contactId) {
    return null;
  }
  const trimmed = runtime.contactId.trim();
  return trimmed.length > 0 ? trimmed : null;
};

export const isSessionMatchedContactAndProject = (
  session: Session | Record<string, any> | null | undefined,
  contact: ContactSessionRef,
  projectId: string | null | undefined,
): boolean => {
  if (!session || !isSessionActive(session)) {
    return false;
  }

  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  const contactId = typeof runtime?.contactId === 'string' ? runtime.contactId.trim() : '';
  const contactAgentId = typeof runtime?.contactAgentId === 'string' ? runtime.contactAgentId.trim() : '';

  if (contactId) {
    if (contactId !== contact.id) {
      return false;
    }
  } else if (contactAgentId) {
    if (contactAgentId !== contact.agentId) {
      return false;
    }
  } else {
    return false;
  }

  const normalizedProjectId = normalizeProjectScopeId(projectId);
  const sessionProjectId = resolveSessionProjectScopeId(session);
  return sessionProjectId === normalizedProjectId;
};

export const findLatestMatchedSession = (
  sessions: Session[],
  contact: ContactSessionRef,
  projectId: string | null | undefined,
): Session | null => {
  const candidates = (sessions || []).filter((session: Session) =>
    isSessionMatchedContactAndProject(session, contact, projectId),
  );
  if (candidates.length === 0) {
    return null;
  }
  candidates.sort((left, right) => resolveSessionTimestamp(right) - resolveSessionTimestamp(left));
  return candidates[0] || null;
};
