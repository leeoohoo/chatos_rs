// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ContactRecord } from '../../types';
import type { ProjectContactLinkResponse } from '../api/client/types';
import type { ContactItem, ProjectContactLink } from '../../components/projectExplorer/teamMembers/types';

export const normalizeProjectMemberContacts = (value: unknown): ContactItem[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  const out: ContactItem[] = [];
  for (const item of value) {
    const id = typeof item?.id === 'string' ? item.id.trim() : '';
    const agentId = typeof item?.agentId === 'string' ? item.agentId.trim() : '';
    if (!id || !agentId) {
      continue;
    }
    const name = typeof item?.name === 'string' && item.name.trim()
      ? item.name.trim()
      : id;
    const rawTaskRunner = item?.taskRunner;
    const taskRunner = rawTaskRunner && typeof rawTaskRunner === 'object'
      ? {
          enabled: rawTaskRunner.enabled === true,
          baseUrl: typeof rawTaskRunner.baseUrl === 'string' ? rawTaskRunner.baseUrl : '',
          username: typeof rawTaskRunner.username === 'string' ? rawTaskRunner.username : '',
          hasPassword: rawTaskRunner.hasPassword === true,
        }
      : undefined;
    out.push({ id, agentId, name, taskRunner });
  }
  return out;
};

export const normalizeProjectMemberContactsFromRecords = (
  contacts: ContactRecord[] | undefined | null,
): ContactItem[] => normalizeProjectMemberContacts(contacts || []);

const readStringField = (value: unknown, key: string): string => {
  if (!value || typeof value !== 'object') {
    return '';
  }
  const raw = (value as Record<string, unknown>)[key];
  return typeof raw === 'string' ? raw.trim() : '';
};

const readFirstStringField = (value: unknown, keys: string[]): string => {
  for (const key of keys) {
    const found = readStringField(value, key);
    if (found) {
      return found;
    }
  }
  return '';
};

export const normalizeProjectContactLinks = (
  value: ProjectContactLinkResponse[] | unknown,
): ProjectContactLink[] => {
  const deduped = new Map<string, ProjectContactLink>();
  for (const item of Array.isArray(value) ? value : []) {
    const contactId = readFirstStringField(item, ['contact_id', 'contactId']);
    const agentId = readFirstStringField(item, ['agent_id', 'agentId']);
    if (!contactId || !agentId) {
      continue;
    }
    const name = readFirstStringField(item, ['agent_name_snapshot', 'agentNameSnapshot']) || contactId;
    const latestSessionId = readFirstStringField(item, ['latest_session_id', 'latestSessionId']) || null;
    const lastMessageAt = readFirstStringField(item, ['last_message_at', 'lastMessageAt']) || null;
    const ts = new Date(
      lastMessageAt
      || readFirstStringField(item, ['updated_at', 'updatedAt'])
      || readFirstStringField(item, ['last_bound_at', 'lastBoundAt'])
      || Date.now().toString(),
    ).getTime();
    const updatedAt = Number.isFinite(ts) ? ts : 0;
    const current = deduped.get(contactId);
    if (!current || updatedAt >= current.updatedAt) {
      deduped.set(contactId, {
        contactId,
        agentId,
        name,
        latestSessionId,
        lastMessageAt,
        updatedAt,
      });
    }
  }
  return Array.from(deduped.values()).sort((left, right) => right.updatedAt - left.updatedAt);
};
