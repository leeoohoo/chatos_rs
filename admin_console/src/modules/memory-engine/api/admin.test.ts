// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { client } from './client';
import { adminApi } from './admin';

vi.mock('./client', () => ({
  client: {
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    delete: vi.fn(),
  },
}));

describe('adminApi', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('loads the agent directory and encodes agent subjects in project paths', async () => {
    vi.mocked(client.get)
      .mockResolvedValueOnce({ data: { items: [{ agent_id: 'jiguli' }] } } as never)
      .mockResolvedValueOnce({ data: { items: [{ project_id: 'project-1' }] } } as never)
      .mockResolvedValueOnce({ data: { items: [{ conversation_id: 'conversation-1' }] } } as never);

    await expect(
      adminApi.listAgents({ tenant_id: 'tenant-1', source_id: 'chatos', agent_id: 'jiguli' }),
    ).resolves.toEqual([{ agent_id: 'jiguli' }]);
    expect(client.get).toHaveBeenNthCalledWith(1, '/admin/agents', {
      params: {
        tenant_id: 'tenant-1',
        source_id: 'chatos',
        agent_id: 'jiguli',
        limit: 100,
        offset: 0,
      },
    });

    await expect(
      adminApi.listAgentProjects('agent:review/agent', {
        tenant_id: 'tenant-1',
        source_id: 'chatos',
      }),
    ).resolves.toEqual([{ project_id: 'project-1' }]);
    expect(client.get).toHaveBeenNthCalledWith(
      2,
      '/admin/agents/agent%3Areview%2Fagent/projects',
      { params: { tenant_id: 'tenant-1', source_id: 'chatos' } },
    );

    await expect(
      adminApi.listAgentConversations('agent:review/agent', {
        tenant_id: 'tenant-1',
        source_id: 'chatos',
      }),
    ).resolves.toEqual([{ conversation_id: 'conversation-1' }]);
    expect(client.get).toHaveBeenNthCalledWith(
      3,
      '/admin/agents/agent%3Areview%2Fagent/conversations',
      { params: { tenant_id: 'tenant-1', source_id: 'chatos' } },
    );
  });

  it('normalizes source and policy responses with stable defaults', async () => {
    vi.mocked(client.get)
      .mockResolvedValueOnce({
        data: {
          items: [
            {
              id: 'src-1',
              source_id: 'source-a',
              source_type: 'sdk_system',
              name: 'Source A',
              created_at: '2026-05-21T00:00:00Z',
              updated_at: '2026-05-21T00:00:00Z',
            },
          ],
        },
      } as never)
      .mockResolvedValueOnce({
        data: {
          configuration_status: 'ready',
          items: [
            {
              job_type: 'thread_summary',
              updated_at: '2026-05-21T00:00:00Z',
            },
          ],
        },
      } as never);

    await expect(adminApi.listSources()).resolves.toEqual([
      {
        id: 'src-1',
        tenant_id: null,
        source_id: 'source-a',
        source_type: 'sdk_system',
        name: 'Source A',
        description: null,
        config: null,
        status: 'active',
        sdk_enabled: false,
        secret_key_hint: null,
        key_last_rotated_at: null,
        created_at: '2026-05-21T00:00:00Z',
        updated_at: '2026-05-21T00:00:00Z',
      },
    ]);

    await expect(adminApi.listJobPolicies()).resolves.toEqual({configuration_status: 'ready', items: [
      {
        job_type: 'thread_summary',
        max_model_input_tokens: null,
        model_config_id: null, provider: null, model: null,
        model_capability_revision: null, context_window_tokens: null, output_reserve_tokens: null,
        updated_at: '2026-05-21T00:00:00Z',
      },
    ]});
  });

  it('preserves an unselected summary model as an explicit configuration state', async () => {
    vi.mocked(client.get).mockResolvedValue({data: {
      items: [], configuration_status: 'model_not_selected',
    }} as never);
    await expect(adminApi.listJobPolicies()).resolves.toEqual({
      items: [], configuration_status: 'model_not_selected',
    });
  });

  it('uses dedicated reliability read, dry-run, audit and rearm endpoints', async () => {
    const scope = { tenant_id: 'tenant-1', source_id: 'source-1', limit: 500 };
    vi.mocked(client.get)
      .mockResolvedValueOnce({ data: { finding_count: 0, alerts: [] } } as never)
      .mockResolvedValueOnce({ data: [{ stream: 'rebuild' }] } as never)
      .mockResolvedValueOnce({ data: [{ action_kind: 'rearm' }] } as never);
    vi.mocked(client.post)
      .mockResolvedValueOnce({ data: { thread_id: 'thread-1', selected: [], meta: {} } } as never)
      .mockResolvedValueOnce({ data: { operation_id: 'operation-1' } } as never)
      .mockResolvedValueOnce({ data: { accepted: true, running: true, job_run_id: 'job-1' } } as never)
      .mockResolvedValue({ data: { operation_id: 'operation-2' } } as never);

    await expect(adminApi.getReliabilityDashboard(scope)).resolves.toEqual({ finding_count: 0, alerts: [] });
    expect(client.get).toHaveBeenNthCalledWith(1, '/admin/reliability/dashboard', { params: scope });
    await expect(adminApi.listReliabilityQueues(scope)).resolves.toEqual([
      { stream: 'rebuild' },
    ]);
    expect(client.get).toHaveBeenNthCalledWith(2, '/admin/reliability/queues', { params: scope });
    await expect(adminApi.listReliabilityOperatorActions(scope)).resolves.toEqual([
      { action_kind: 'rearm' },
    ]);
    expect(client.get).toHaveBeenNthCalledWith(3, '/admin/reliability/operator-actions', {
      params: scope,
    });

    const dryRun = {
      tenant_id: 'tenant-1',
      source_id: 'source-1',
      thread_id: 'thread-1',
      model_capability_revision: 'capability-1',
      policy: {
        max_memory_tokens: 12000,
        max_recent_record_tokens: 4000,
        max_thread_summary_tokens: 4000,
        max_project_memory_tokens: 3000,
        max_agent_memory_tokens: 2000,
      },
    };
    await adminApi.composeReliabilityDryRun(dryRun);
    expect(client.post).toHaveBeenNthCalledWith(1, '/admin/reliability/compose-dry-run', dryRun);

    const rearm = {
      tenant_id: 'tenant-1',
      source_id: 'source-1',
      thread_id: 'thread-1',
      project_partition_key: 'project:alpha',
      rebuild_kind: 'thread_l0',
      reason: 'operator confirmed recovery',
    };
    await adminApi.rearmPartitionRebuild(rearm);
    expect(client.post).toHaveBeenNthCalledWith(
      2,
      '/admin/reliability/rebuild-backlog/rearm',
      rearm,
    );

    const resummarize = {
      tenant_id: 'tenant-1',
      source_id: 'source-1',
      summary_id: 'summary-1',
      reason: 'correct outdated summary',
    };
    await adminApi.resummarizeCoveredRange(resummarize);
    expect(client.post).toHaveBeenNthCalledWith(
      3,
      '/admin/reliability/resummarize',
      resummarize,
    );
    await adminApi.retireSummary(resummarize);
    expect(client.post).toHaveBeenNthCalledWith(4, '/admin/reliability/retire-summary', resummarize);
    const restore = { tenant_id: 'tenant-1', source_id: 'source-1', record_id: 'record-1', reason: 'repair missing coverage' };
    await adminApi.restoreRecordPending(restore);
    expect(client.post).toHaveBeenNthCalledWith(5, '/admin/reliability/restore-pending', restore);
    const queueRearm = { tenant_id: 'tenant-1', source_id: 'source-1', stream: 'derived_event', item_id: 'event-1', version: 2, reason: 'retry after repair' };
    await adminApi.rearmReliabilityQueue(queueRearm);
    expect(client.post).toHaveBeenNthCalledWith(6, '/admin/reliability/queues/rearm', queueRearm);
  });
});
