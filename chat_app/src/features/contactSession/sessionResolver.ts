import type { Session } from '../../types';
import { readSessionRuntimeFromMetadata } from '../../lib/store/helpers/sessionRuntime';

// 这里解析的是“联系人 + 项目作用域”与其背后的承载 session 的映射关系。
// session 仍然存在，但只是消息/流式/历史记录的技术容器，不再是业务主体。
export interface ContactProjectScopeRef {
  id: string;
  agentId: string;
}

type SessionLike = {
  id?: string;
  title?: string;
  projectId?: string | null;
  project_id?: string | null;
  metadata?: Session['metadata'];
  updatedAt?: string | Date;
  updated_at?: string | Date;
  createdAt?: string | Date;
  created_at?: string | Date;
  archived?: boolean;
  status?: string;
};

export const normalizeProjectScopeId = (projectId: string | null | undefined): string => {
  const trimmed = typeof projectId === 'string' ? projectId.trim() : '';
  return trimmed.length > 0 ? trimmed : '0';
};

export const resolveProjectScopeIdFromRecord = (
  session: SessionLike | null | undefined,
): string => {
  if (!session) {
    return '0';
  }
  const rawProjectId = typeof session.projectId === 'string'
    ? session.projectId.trim()
    : (typeof session.project_id === 'string'
      ? session.project_id.trim()
      : '');
  if (rawProjectId.length > 0) {
    return normalizeProjectScopeId(rawProjectId);
  }
  const runtime = readSessionRuntimeFromMetadata(session.metadata);
  return normalizeProjectScopeId(runtime?.projectId ?? null);
};

export const resolveSessionTimestamp = (
  session: SessionLike | null | undefined,
): number => {
  if (!session) {
    return 0;
  }
  const raw = session.updatedAt
    ?? session.updated_at
    ?? session.createdAt
    ?? session.created_at
    ?? Date.now();
  const ts = new Date(raw).getTime();
  return Number.isFinite(ts) ? ts : 0;
};

export const isSessionActive = (
  session: SessionLike | null | undefined,
): boolean => {
  if (!session) {
    return false;
  }
  const archived = session.archived === true;
  const status = typeof session.status === 'string'
    ? session.status.toLowerCase()
    : '';
  return !archived && status !== 'archived' && status !== 'archiving';
};

export const resolveContactAgentIdFromScopeRecord = (
  session: SessionLike | null | undefined,
): string | null => {
  if (!session) {
    return null;
  }
  const runtime = readSessionRuntimeFromMetadata(session.metadata);
  if (!runtime?.contactAgentId) {
    return null;
  }
  const trimmed = runtime.contactAgentId.trim();
  return trimmed.length > 0 ? trimmed : null;
};

export const resolveContactIdFromScopeRecord = (
  session: SessionLike | null | undefined,
): string | null => {
  if (!session) {
    return null;
  }
  const runtime = readSessionRuntimeFromMetadata(session.metadata);
  if (!runtime?.contactId) {
    return null;
  }
  const trimmed = runtime.contactId.trim();
  return trimmed.length > 0 ? trimmed : null;
};

export const isRecordMatchedContactProjectScope = (
  session: SessionLike | null | undefined,
  contact: ContactProjectScopeRef,
  projectId: string | null | undefined,
): boolean => {
  if (!session || !isSessionActive(session)) {
    return false;
  }

  const runtime = readSessionRuntimeFromMetadata(session.metadata);
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
  const sessionProjectId = resolveProjectScopeIdFromRecord(session);
  return sessionProjectId === normalizedProjectId;
};

export const findLatestRecordForContactProjectScope = (
  sessions: Session[],
  contact: ContactProjectScopeRef,
  projectId: string | null | undefined,
): Session | null => {
  const candidates = (sessions || []).filter((session: Session) =>
    isRecordMatchedContactProjectScope(session, contact, projectId),
  );
  if (candidates.length === 0) {
    return null;
  }
  candidates.sort((left, right) => resolveSessionTimestamp(right) - resolveSessionTimestamp(left));
  return candidates[0] || null;
};

export type ContactSessionRef = ContactProjectScopeRef;
export const resolveSessionProjectScopeId = resolveProjectScopeIdFromRecord;
export const resolveContactAgentIdFromSession = resolveContactAgentIdFromScopeRecord;
export const resolveContactIdFromSession = resolveContactIdFromScopeRecord;
export const isSessionMatchedContactAndProject = isRecordMatchedContactProjectScope;
export const findLatestMatchedSession = findLatestRecordForContactProjectScope;
