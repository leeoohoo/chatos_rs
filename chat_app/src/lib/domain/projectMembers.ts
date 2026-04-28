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
    out.push({ id, agentId, name });
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

export const normalizeProjectContactLinks = (
  value: ProjectContactLinkResponse[] | unknown,
): ProjectContactLink[] => {
  const deduped = new Map<string, ProjectContactLink>();
  for (const item of Array.isArray(value) ? value : []) {
    const contactId = readStringField(item, 'contact_id');
    const agentId = readStringField(item, 'agent_id');
    if (!contactId || !agentId) {
      continue;
    }
    const name = readStringField(item, 'agent_name_snapshot') || contactId;
    const ts = new Date(
      readStringField(item, 'updated_at')
      || readStringField(item, 'last_bound_at')
      || Date.now().toString(),
    ).getTime();
    const updatedAt = Number.isFinite(ts) ? ts : 0;
    const current = deduped.get(contactId);
    if (!current || updatedAt >= current.updatedAt) {
      deduped.set(contactId, { contactId, agentId, name, updatedAt });
    }
  }
  return Array.from(deduped.values()).sort((left, right) => right.updatedAt - left.updatedAt);
};
