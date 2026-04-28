import { describe, expect, it } from 'vitest';

import type { AgentConfig, ContactRecord, Session } from '../../types';
import { resolveCurrentAgent } from './currentAgent';

const createAgent = (id: string, name: string): AgentConfig => ({
  id,
  name,
  description: '',
  ai_model_config_id: 'model_1',
  enabled: true,
  role_definition: '',
  skills: [],
  skill_ids: [],
  default_skill_ids: [],
  plugin_sources: [],
  runtime_plugins: [],
  runtime_skills: [],
  mcp_policy: null,
  project_policy: null,
  createdAt: new Date('2026-04-23T00:00:00.000Z'),
  updatedAt: new Date('2026-04-23T00:00:00.000Z'),
});

const createSession = (
  overrides: Partial<Session> = {},
): Session => ({
  id: 'session_1',
  title: 'Alice',
  createdAt: new Date('2026-04-23T00:00:00.000Z'),
  updatedAt: new Date('2026-04-23T00:00:00.000Z'),
  messageCount: 0,
  tokenUsage: 0,
  pinned: false,
  archived: false,
  metadata: {},
  ...overrides,
});

describe('resolveCurrentAgent', () => {
  it('prefers the explicitly selected agent id', () => {
    const selected = createAgent('agent_selected', 'Selected Agent');
    const resolved = resolveCurrentAgent({
      currentSession: createSession(),
      contacts: [],
      agents: [selected, createAgent('agent_other', 'Other Agent')],
      selectedAgentId: 'agent_selected',
    });

    expect(resolved).toBe(selected);
  });

  it('matches the runtime contact and returns a loaded agent', () => {
    const agent = createAgent('agent_contact', 'Contact Agent');
    const contact = {
      id: 'contact_1',
      agentId: 'agent_contact',
      name: 'Alice',
      createdAt: new Date('2026-04-23T00:00:00.000Z'),
      updatedAt: new Date('2026-04-23T00:00:00.000Z'),
    } as ContactRecord;

    const resolved = resolveCurrentAgent({
      currentSession: createSession({
        metadata: {
          runtime: {
            contactId: 'contact_1',
          },
        },
      }),
      contacts: [contact],
      agents: [agent],
      selectedAgentId: null,
    });

    expect(resolved).toBe(agent);
  });

  it('creates a fallback agent when the contact agent is not loaded', () => {
    const contact = {
      id: 'contact_1',
      agentId: 'agent_missing',
      name: 'Alice',
      createdAt: new Date('2026-04-23T00:00:00.000Z'),
      updatedAt: new Date('2026-04-23T00:00:00.000Z'),
    } as ContactRecord;

    const resolved = resolveCurrentAgent({
      currentSession: createSession(),
      contacts: [contact],
      agents: [],
      selectedAgentId: null,
    });

    expect(resolved).toMatchObject({
      id: 'agent_missing',
      name: 'Alice',
    });
  });
});
