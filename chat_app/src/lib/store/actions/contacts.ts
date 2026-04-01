import type ApiClient from '../../api/client';
import type { ContactRecord } from '../types';
import { normalizeContact } from '../helpers/contacts';

interface Deps {
  set: any;
  get: any;
  client: ApiClient;
  getUserIdParam: () => string;
}

const upsertContactRecord = (list: ContactRecord[], next: ContactRecord): ContactRecord[] => {
  const index = list.findIndex((item) => item.id === next.id);
  if (index === -1) {
    return [next, ...list];
  }
  const copied = [...list];
  copied[index] = next;
  return copied;
};

export function createContactActions({ set, get, client, getUserIdParam }: Deps) {
  return {
    loadContacts: async () => {
      try {
        const uid = getUserIdParam();
        const list = await client.getContacts(uid, { limit: 2000, offset: 0 });
        const normalized = (Array.isArray(list) ? list : [])
          .map(normalizeContact)
          .filter((item): item is ContactRecord => !!item)
          .filter((item) => item.status === '' || item.status === 'active')
          .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime());
        set((state: any) => {
          state.contacts = normalized;
        });
        return normalized;
      } catch (error) {
        console.error('Failed to load contacts:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load contacts';
        });
        return [];
      }
    },

    createContact: async (agentId: string, agentNameSnapshot?: string) => {
      const trimmedAgentId = agentId.trim();
      if (!trimmedAgentId) {
        throw new Error('agentId is required');
      }
      const uid = getUserIdParam();
      const response = await client.createContact({
        user_id: uid,
        agent_id: trimmedAgentId,
        agent_name_snapshot: agentNameSnapshot?.trim() || undefined,
      });
      const rawContact = (
        response && typeof response === 'object' && 'contact' in response
          ? response.contact
          : response
      );
      const normalized = normalizeContact(rawContact);
      if (!normalized) {
        throw new Error('create contact returned invalid payload');
      }
      set((state: any) => {
        state.contacts = upsertContactRecord(state.contacts || [], normalized)
          .filter((item: ContactRecord) => item.status === '' || item.status === 'active')
          .sort((a: ContactRecord, b: ContactRecord) => b.updatedAt.getTime() - a.updatedAt.getTime());
      });
      return normalized;
    },

    deleteContact: async (contactId: string) => {
      const trimmed = contactId.trim();
      if (!trimmed) {
        return;
      }
      await client.deleteContact(trimmed);
      set((state: any) => {
        state.contacts = (state.contacts || []).filter((item: ContactRecord) => item.id !== trimmed);
      });
    },

    getContactByAgentId: (agentId: string) => {
      const trimmed = agentId.trim();
      if (!trimmed) {
        return null;
      }
      const contacts: ContactRecord[] = get().contacts || [];
      return contacts.find((item) => item.agentId === trimmed) || null;
    },
  };
}
