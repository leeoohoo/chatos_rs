// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import type { ChatStoreDraft, ChatStoreShape } from '../types';
import { createContactActions } from './contacts';

describe('loadContacts', () => {
  it('lets a forced refresh supersede an older in-flight contact request', async () => {
    const state = {
      contacts: [],
      error: null,
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    let resolveInitial: ((value: unknown[]) => void) | undefined;
    const getContacts = vi.fn()
      .mockImplementationOnce(() => new Promise<unknown[]>((resolve) => {
        resolveInitial = resolve;
      }))
      .mockResolvedValueOnce([{
        id: 'contact_fresh',
        agent_id: 'agent_fresh',
        agent_name_snapshot: 'Fresh Contact',
        status: 'active',
        created_at: '2026-07-19T00:00:00.000Z',
        updated_at: '2026-07-19T00:00:00.000Z',
      }]);
    const actions = createContactActions({
      set,
      get: () => state,
      client: { getContacts } as never,
      getUserIdParam: () => 'user_force_contacts',
    });

    const initial = actions.loadContacts();
    await Promise.resolve();
    await actions.loadContacts({ force: true });
    resolveInitial?.([{
      id: 'contact_stale',
      agent_id: 'agent_stale',
      agent_name_snapshot: 'Stale Contact',
      status: 'active',
      created_at: '2026-07-18T00:00:00.000Z',
      updated_at: '2026-07-18T00:00:00.000Z',
    }]);
    await initial;

    expect(getContacts).toHaveBeenCalledTimes(2);
    expect(state.contacts.map((contact) => contact.id)).toEqual(['contact_fresh']);
  });
});
