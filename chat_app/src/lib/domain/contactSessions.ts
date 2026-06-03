import type { Session } from '../../types';
import { asRecord, readTrimmedString, readValue } from './normalizerUtils';
import { readSessionRuntimeFromMetadata } from './sessionRuntime';

export interface ContactSessionRef {
  id: string;
  agentId: string;
}

export type MemoryContact = {
  id: string;
  user_id: string;
  agent_id: string;
  agent_name_snapshot?: string | null;
  status?: string | null;
  created_at?: string;
  updated_at?: string;
};

export const normalizeProjectScopeId = (projectId: string | null | undefined): string => {
  const trimmed = typeof projectId === 'string' ? projectId.trim() : '';
  return trimmed.length > 0 ? trimmed : '0';
};

export const resolveSessionProjectScopeId = (
  session: unknown,
): string => {
  const record = asRecord(session);
  if (!record) {
    return '0';
  }
  const rawProjectId = readTrimmedString(record, 'projectId')
    || readTrimmedString(record, 'project_id');
  if (rawProjectId.length > 0) {
    return normalizeProjectScopeId(rawProjectId);
  }
  const runtime = readSessionRuntimeFromMetadata(readValue(record, 'metadata'));
  return normalizeProjectScopeId(runtime?.projectId ?? null);
};

export const resolveSessionTimestamp = (
  session: unknown,
): number => {
  const record = asRecord(session);
  if (!record) {
    return 0;
  }
  const raw = readValue(record, 'updatedAt')
    ?? readValue(record, 'updated_at')
    ?? readValue(record, 'createdAt')
    ?? readValue(record, 'created_at')
    ?? Date.now();
  const ts = new Date(raw as string | number | Date).getTime();
  return Number.isFinite(ts) ? ts : 0;
};

export const isSessionActive = (
  session: unknown,
): boolean => {
  const record = asRecord(session);
  if (!record) {
    return false;
  }
  const archived = readValue(record, 'archived') === true;
  const statusValue = readValue(record, 'status');
  const status = typeof statusValue === 'string'
    ? statusValue.toLowerCase()
    : '';
  return !archived && status !== 'archived' && status !== 'archiving';
};

export const resolveSessionContactIdentity = (session: unknown): {
  contactId: string | null;
  contactAgentId: string | null;
} => {
  const record = asRecord(session);
  if (!record) {
    return { contactId: null, contactAgentId: null };
  }
  const runtime = readSessionRuntimeFromMetadata(readValue(record, 'metadata'));
  const contactId = typeof runtime?.contactId === 'string' ? runtime.contactId.trim() : '';
  const contactAgentId = typeof runtime?.contactAgentId === 'string' ? runtime.contactAgentId.trim() : '';
  return {
    contactId: contactId.length > 0 ? contactId : null,
    contactAgentId: contactAgentId.length > 0 ? contactAgentId : null,
  };
};

export const resolveContactAgentIdFromSession = (
  session: unknown,
): string | null => resolveSessionContactIdentity(session).contactAgentId;

export const resolveContactIdFromSession = (
  session: unknown,
): string | null => resolveSessionContactIdentity(session).contactId;

export const matchSessionContactProjectScope = (
  session: unknown,
  target: {
    contactId?: string | null;
    contactAgentId?: string | null;
    projectId: string;
  },
): boolean => {
  if (!isSessionActive(session)) {
    return false;
  }

  const contactId = typeof target.contactId === 'string' ? target.contactId.trim() : '';
  const contactAgentId = typeof target.contactAgentId === 'string' ? target.contactAgentId.trim() : '';
  const identity = resolveSessionContactIdentity(session);

  let sameContact = false;
  if (contactId) {
    sameContact = identity.contactId === contactId;
  } else if (contactAgentId) {
    sameContact = identity.contactAgentId === contactAgentId;
  }

  if (!sameContact) {
    return false;
  }

  return resolveSessionProjectScopeId(session) === normalizeProjectScopeId(target.projectId);
};

export const isSessionMatchedContactAndProject = (
  session: unknown,
  contact: ContactSessionRef,
  projectId: string | null | undefined,
): boolean => (
  matchSessionContactProjectScope(session, {
    contactId: contact.id,
    contactAgentId: contact.agentId,
    projectId: normalizeProjectScopeId(projectId),
  })
);

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

export const splitSessionsByMappedContacts = (
  sessions: Session[],
  contacts: MemoryContact[],
): {
  matchedSessions: Session[];
  missingContacts: MemoryContact[];
} => {
  const contactsById = new Map(contacts.map((item) => [item.id, item]));
  const contactsByAgentId = new Map<string, MemoryContact[]>();
  for (const contact of contacts) {
    const existing = contactsByAgentId.get(contact.agent_id) || [];
    existing.push(contact);
    contactsByAgentId.set(contact.agent_id, existing);
  }

  const mappedContactIds = new Set<string>();
  const matchedSessions = sessions.filter((session) => {
    if (!isSessionActive(session)) {
      return false;
    }
    const identity = resolveSessionContactIdentity(session);
    if (identity.contactId && contactsById.has(identity.contactId)) {
      mappedContactIds.add(identity.contactId);
      return true;
    }
    if (identity.contactAgentId) {
      const mappedContacts = contactsByAgentId.get(identity.contactAgentId) || [];
      if (mappedContacts.length === 1) {
        mappedContactIds.add(mappedContacts[0].id);
        return true;
      }
    }
    return false;
  });

  const missingContacts = contacts.filter((contact) => {
    if (mappedContactIds.has(contact.id)) {
      return false;
    }
    return true;
  });

  return {
    matchedSessions,
    missingContacts,
  };
};

export const normalizeContactSessions = (sessions: Session[]): Session[] => {
  const byContactProject = new Map<string, Session>();
  for (const session of sessions) {
    const identity = resolveSessionContactIdentity(session);
    const contactKey = identity.contactId || identity.contactAgentId;
    if (!contactKey) {
      continue;
    }
    const key = `${contactKey}::${resolveSessionProjectScopeId(session)}`;
    const existing = byContactProject.get(key);
    if (!existing || resolveSessionTimestamp(session) >= resolveSessionTimestamp(existing)) {
      byContactProject.set(key, session);
    }
  }
  return Array.from(byContactProject.values()).sort(
    (a, b) => resolveSessionTimestamp(b) - resolveSessionTimestamp(a),
  );
};

export const normalizeMemoryContact = (value: unknown): MemoryContact | null => {
  const record = asRecord(value);
  if (!record) {
    return null;
  }
  const id = readTrimmedString(record, 'id');
  const agentId = readTrimmedString(record, 'agent_id');
  const userId = readTrimmedString(record, 'user_id');
  if (!id || !agentId || !userId) {
    return null;
  }
  return {
    id,
    user_id: userId,
    agent_id: agentId,
    agent_name_snapshot: typeof readValue(record, 'agent_name_snapshot') === 'string'
      ? readTrimmedString(record, 'agent_name_snapshot')
      : null,
    status: typeof readValue(record, 'status') === 'string' ? readTrimmedString(record, 'status') : null,
    created_at: typeof readValue(record, 'created_at') === 'string' ? readValue(record, 'created_at') as string : undefined,
    updated_at: typeof readValue(record, 'updated_at') === 'string' ? readValue(record, 'updated_at') as string : undefined,
  };
};
