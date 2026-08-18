// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createLoadSessionActions } from './loadSessions';

describe('createLoadSessionActions', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('loads sessions after refreshing cloud contacts for memory mapping', async () => {
    const state: Record<string, unknown> = {
      activePanel: 'project',
      contacts: [],
      currentSession: null,
      currentSessionId: null,
      error: null,
      isLoading: false,
      sessionAiSelectionBySession: {},
      sessions: [],
      messages: [],
      selectedModelId: null,
      selectedAgentId: null,
      isStreaming: false,
      streamingMessageId: null,
      hasMoreMessages: false,
    };

    const set = (updater: (draft: typeof state) => void) => {
      updater(state);
    };
    const loadContacts = vi.fn(async () => ([
      {
        id: 'contact-1',
        agentId: 'agent-1',
        name: 'Alice',
        status: 'active',
        createdAt: new Date('2026-08-11T00:00:00.000Z'),
        updatedAt: new Date('2026-08-11T00:00:00.000Z'),
      },
    ]));
    const get = () => ({
      ...state,
      loadContacts,
      selectSession: vi.fn(),
    });
    const client = {
      getSessions: vi.fn(async () => [
        {
          id: 'session-1',
          title: 'Desktop Session',
          user_id: 'user-1',
          status: 'active',
          created_at: '2026-08-11T00:00:00.000Z',
          updated_at: '2026-08-11T00:00:00.000Z',
          metadata: {
            chat_runtime: {
              contact_agent_id: 'agent-1',
              project_id: '-1',
            },
          },
        },
      ]),
    } as const;

    const actions = createLoadSessionActions({
      set: set as never,
      get: get as never,
      client: client as never,
      getSessionParams: () => ({ userId: 'user-1', projectId: '-1' }),
    });

    const loaded = await actions.loadSessions({ silent: true });

    expect(loadContacts).toHaveBeenCalledTimes(1);
    expect(client.getSessions).toHaveBeenCalledWith('user-1', '-1', {
      limit: undefined,
      offset: undefined,
    });
    expect(Array.isArray(loaded)).toBe(true);
    expect((loaded as Array<{ id: string }>)).toHaveLength(1);
    expect((loaded as Array<{ id: string }>)[0]?.id).toBe('session-1');
    expect((state.sessions as Array<{ id: string }>)).toHaveLength(1);
    expect((state.sessions as Array<{ id: string }>)[0]?.id).toBe('session-1');
  });
});
