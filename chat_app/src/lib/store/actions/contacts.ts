// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type ApiClient from '../../api/client';
import { ApiRequestError } from '../../api/client/shared';
import type { ContactRecord } from '../types';
import { normalizeContact } from '../helpers/contacts';
import type { ChatStoreDraft, ChatStoreGet, ChatStoreSet } from '../types';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
  getUserIdParam: () => string;
}

interface LoadContactsOptions {
  force?: boolean;
}

interface ContactsCacheEntry {
  contacts: ContactRecord[];
  stale: boolean;
}

interface ContactsClientCacheState {
  cache: Map<string, ContactsCacheEntry>;
  inflight: Map<string, Promise<ContactRecord[]>>;
  detailCache: Map<string, ContactRecord>;
  detailInflight: Map<string, Promise<ContactRecord | null>>;
}

const contactsClientCaches = new WeakMap<ApiClient, ContactsClientCacheState>();

const normalizeUserId = (userId: string): string => String(userId || '').trim();

const getOrCreateClientCacheState = (apiClient: ApiClient): ContactsClientCacheState => {
  const existing = contactsClientCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: ContactsClientCacheState = {
    cache: new Map(),
    inflight: new Map(),
    detailCache: new Map(),
    detailInflight: new Map(),
  };
  contactsClientCaches.set(apiClient, next);
  return next;
};

const normalizeContactId = (contactId: string): string => String(contactId || '').trim();

const upsertContactRecord = (list: ContactRecord[], next: ContactRecord): ContactRecord[] => {
  const index = list.findIndex((item) => item.id === next.id);
  if (index === -1) {
    return [next, ...list];
  }
  const copied = [...list];
  copied[index] = next;
  return copied;
};

const markContactsCacheStale = (apiClient: ApiClient, userId?: string | null) => {
  const cacheState = getOrCreateClientCacheState(apiClient);
  const normalizedUserId = normalizeUserId(String(userId || ''));
  if (normalizedUserId) {
    const cached = cacheState.cache.get(normalizedUserId);
    if (cached) {
      cacheState.cache.set(normalizedUserId, {
        ...cached,
        stale: true,
      });
    }
    return;
  }
  cacheState.cache.forEach((entry, key) => {
    cacheState.cache.set(key, {
      ...entry,
      stale: true,
    });
  });
};

export function createContactActions({ set, get, client, getUserIdParam }: Deps) {
  const syncLoadedContacts = (userId: string, contacts: ContactRecord[]) => {
    const cacheState = getOrCreateClientCacheState(client);
    cacheState.cache.set(normalizeUserId(userId), {
      contacts,
      stale: false,
    });
    contacts.forEach((contact) => {
      cacheState.detailCache.set(contact.id, contact);
    });
  };

  const upsertContactCaches = (contact: ContactRecord) => {
    const cacheState = getOrCreateClientCacheState(client);
    cacheState.detailCache.set(contact.id, contact);
    cacheState.cache.forEach((entry, key) => {
      cacheState.cache.set(key, {
        contacts: upsertContactRecord(entry.contacts, contact)
          .filter((item: ContactRecord) => item.status === '' || item.status === 'active')
          .sort((a: ContactRecord, b: ContactRecord) => b.updatedAt.getTime() - a.updatedAt.getTime()),
        stale: false,
      });
    });
  };

  const removeContactCaches = (contactId: string) => {
    const trimmed = contactId.trim();
    if (!trimmed) {
      return;
    }
    const cacheState = getOrCreateClientCacheState(client);
    cacheState.detailCache.delete(trimmed);
    cacheState.detailInflight.delete(trimmed);
    cacheState.cache.forEach((entry, key) => {
      cacheState.cache.set(key, {
        contacts: entry.contacts.filter((item) => item.id !== trimmed),
        stale: false,
      });
    });
  };

  return {
    applyRealtimeContactSnapshot: (contactPayload: ContactRecord | unknown) => {
      const normalized = normalizeContact(contactPayload);
      if (!normalized) {
        return null;
      }
      upsertContactCaches(normalized);
      set((state: ChatStoreDraft) => {
        state.contacts = upsertContactRecord(state.contacts || [], normalized)
          .filter((item: ContactRecord) => item.status === '' || item.status === 'active')
          .sort((a: ContactRecord, b: ContactRecord) => b.updatedAt.getTime() - a.updatedAt.getTime());
      });
      return normalized;
    },

    loadContacts: async (options?: LoadContactsOptions) => {
      try {
        const uid = getUserIdParam();
        const cacheKey = normalizeUserId(uid);
        const cacheState = getOrCreateClientCacheState(client);
        const cached = cacheState.cache.get(cacheKey);
        if (!options?.force && cached && !cached.stale) {
          const normalized = cached.contacts;
          set((state: ChatStoreDraft) => {
            state.contacts = normalized;
          });
          return normalized;
        }

        let inflight = cacheState.inflight.get(cacheKey);
        if (!inflight) {
          inflight = client.getContacts(uid, { limit: 2000, offset: 0 })
            .then((list) => (Array.isArray(list) ? list : [])
              .map(normalizeContact)
              .filter((item): item is ContactRecord => !!item)
              .filter((item) => item.status === '' || item.status === 'active')
              .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime()))
            .then((normalized) => {
              syncLoadedContacts(uid, normalized);
              return normalized;
            })
            .finally(() => {
              cacheState.inflight.delete(cacheKey);
            });
          cacheState.inflight.set(cacheKey, inflight);
        }

        const normalized = await inflight;
        set((state: ChatStoreDraft) => {
          state.contacts = normalized;
        });
        return normalized;
      } catch (error) {
        console.error('Failed to load contacts:', error);
        set((state: ChatStoreDraft) => {
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
      upsertContactCaches(normalized);
      set((state: ChatStoreDraft) => {
        state.contacts = upsertContactRecord(state.contacts || [], normalized)
          .filter((item: ContactRecord) => item.status === '' || item.status === 'active')
          .sort((a: ContactRecord, b: ContactRecord) => b.updatedAt.getTime() - a.updatedAt.getTime());
      });
      return normalized;
    },

    updateContactTaskRunnerConfig: async (
      contactId: string,
      config: {
        enabled: boolean;
        baseUrl?: string;
        agentAccountId?: string;
        username: string;
        password?: string;
        clearPassword?: boolean;
      },
    ) => {
      const trimmed = contactId.trim();
      if (!trimmed) {
        throw new Error('contactId is required');
      }
      const password = config.password?.trim();
      const updated = await client.updateContactTaskRunnerConfig(trimmed, {
        enabled: config.enabled,
        base_url: config.baseUrl?.trim() || null,
        task_runner_agent_account_id: config.agentAccountId?.trim() || null,
        username: config.username.trim() || null,
        password: password || undefined,
        clear_password: config.clearPassword,
      });
      const normalized = normalizeContact(updated);
      if (!normalized) {
        throw new Error('update contact returned invalid payload');
      }
      upsertContactCaches(normalized);
      set((state: ChatStoreDraft) => {
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
      removeContactCaches(trimmed);
      set((state: ChatStoreDraft) => {
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

    markContactsStale: (userId?: string | null) => {
      markContactsCacheStale(client, userId);
    },

    refreshContactById: async (contactId: string) => {
      const normalizedContactId = normalizeContactId(contactId);
      if (!normalizedContactId) {
        return null;
      }
      try {
        const cacheState = getOrCreateClientCacheState(client);
        let inflight = cacheState.detailInflight.get(normalizedContactId);
        if (!inflight) {
          inflight = client.getContact(normalizedContactId)
            .then((contact) => normalizeContact(contact))
            .then((normalized) => {
              if (!normalized) {
                throw new Error('contact payload invalid');
              }
              upsertContactCaches(normalized);
              return normalized;
            })
            .catch((error) => {
              if (error instanceof ApiRequestError && error.status === 404) {
                removeContactCaches(normalizedContactId);
                return null;
              }
              throw error;
            })
            .finally(() => {
              cacheState.detailInflight.delete(normalizedContactId);
            });
          cacheState.detailInflight.set(normalizedContactId, inflight);
        }

        const contact = await inflight;
        if (!contact) {
          set((state: ChatStoreDraft) => {
            state.contacts = (state.contacts || []).filter((item: ContactRecord) => item.id !== normalizedContactId);
          });
          return null;
        }

        set((state: ChatStoreDraft) => {
          state.contacts = upsertContactRecord(state.contacts || [], contact)
            .filter((item: ContactRecord) => item.status === '' || item.status === 'active')
            .sort((a: ContactRecord, b: ContactRecord) => b.updatedAt.getTime() - a.updatedAt.getTime());
        });
        return contact;
      } catch (error) {
        console.error('Failed to refresh contact detail:', error);
        return null;
      }
    },

    removeContactLocally: (contactId: string) => {
      const trimmed = contactId.trim();
      if (!trimmed) {
        return;
      }
      removeContactCaches(trimmed);
      set((state: ChatStoreDraft) => {
        state.contacts = (state.contacts || []).filter((item: ContactRecord) => item.id !== trimmed);
      });
    },
  };
}
