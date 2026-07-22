// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from 'vitest';

import { readSessionRuntimeFromMetadata } from '../../domain/sessionRuntime';
import { LocalRuntimeSessionClient } from './sessionClient';

afterEach(() => {
  delete window.chatosLocalRuntime;
});

describe('LocalRuntimeSessionClient contact identity', () => {
  it('persists the contact id when creating a local session', async () => {
    const apiRequest = vi.fn(async (request: { body?: string | null }) => {
      void request;
      return {
        status: 200,
        ok: true,
        body: JSON.stringify({
          id: 'lc_session_1',
          project_id: 'project-1',
          owner_user_id: 'user-1',
          title: 'Contact One',
          contact_id: 'contact-1',
          selected_model_id: 'model-1',
          selected_agent_id: 'agent-1',
          status: 'active',
          message_count: 0,
          created_at: '2026-07-21T00:00:00Z',
          updated_at: '2026-07-21T00:00:00Z',
        }),
      };
    });
    window.chatosLocalRuntime = { apiRequest };
    const client = new LocalRuntimeSessionClient();

    const session = await client.createSession({
      id: 'client-session-1',
      title: 'Contact One',
      user_id: 'user-1',
      project_id: 'project-1',
      metadata: {
        chat_runtime: {
          contact_agent_id: 'agent-1',
          selected_model_id: 'model-1',
          project_id: 'project-1',
        },
        contact: {
          contact_id: 'contact-1',
          agent_id: 'agent-1',
        },
      },
    });

    expect(JSON.parse(String(apiRequest.mock.calls[0]?.[0]?.body))).toMatchObject({
      contact_id: 'contact-1',
      selected_agent_id: 'agent-1',
      selected_model_id: 'model-1',
    });
    expect(readSessionRuntimeFromMetadata(session.metadata)).toMatchObject({
      contactId: 'contact-1',
      contactAgentId: 'agent-1',
      projectId: 'project-1',
    });
  });

  it('restores persisted contact identity when listing local sessions', async () => {
    window.chatosLocalRuntime = {
      apiRequest: vi.fn(async () => ({
        status: 200,
        ok: true,
        body: JSON.stringify([{
          id: 'lc_session_2',
          project_id: 'project-1',
          owner_user_id: 'user-1',
          title: 'Contact Two',
          contact_id: 'contact-2',
          selected_model_id: 'model-2',
          selected_agent_id: 'agent-2',
          status: 'active',
          message_count: 2,
          created_at: '2026-07-21T00:00:00Z',
          updated_at: '2026-07-21T00:01:00Z',
        }]),
      })),
    };
    const client = new LocalRuntimeSessionClient();

    const [session] = await client.getSessions('project-1');

    expect(readSessionRuntimeFromMetadata(session.metadata)).toMatchObject({
      contactId: 'contact-2',
      contactAgentId: 'agent-2',
      projectId: 'project-1',
    });
  });
});
