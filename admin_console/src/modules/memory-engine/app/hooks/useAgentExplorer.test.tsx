// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '../../api';
import { renderHook } from '../../test/renderHook';
import { buildAgentFilters, useAgentExplorer } from './useAgentExplorer';

vi.mock('../../api', () => ({
  api: {
    listAgents: vi.fn(),
    listAgentProjects: vi.fn(),
    listSubjectMemories: vi.fn(),
    listThreads: vi.fn(),
    listThreadRecords: vi.fn(),
    listThreadSummaries: vi.fn(),
  },
}));

const agent = {
  tenant_id: 'tenant-1',
  source_id: 'chatos',
  agent_subject_id: 'agent:main-chat',
  agent_id: 'main-chat',
  display_name: '主对话 Agent',
  thread_count: 1,
  project_count: 1,
  memory_count: 1,
  updated_at: '2026-11-04T00:00:00Z',
};

const project = {
  tenant_id: 'tenant-1',
  source_id: 'chatos',
  project_subject_id: 'project:project-1',
  project_id: 'project-1',
  display_name: 'Memory Engine',
  thread_count: 1,
  memory_count: 1,
  updated_at: '2026-11-04T00:00:00Z',
};

const thread = {
  id: 'thread-1',
  tenant_id: 'tenant-1',
  source_id: 'chatos',
  subject_id: 'agent:main-chat',
  thread_type: 'client_agent',
  labels: ['memory-schema:v2', 'agent:main-chat', 'project:project-1'],
  status: 'active',
  summary_status: 'idle',
  pending_record_count: 0,
  pending_summary_tokens: 0,
  created_at: '2026-11-04T00:00:00Z',
  updated_at: '2026-11-04T00:00:00Z',
};

const agentMemory = {
  id: 'memory-agent',
  tenant_id: 'tenant-1',
  source_id: 'chatos',
  subject_id: 'agent:main-chat',
  memory_key: 'agent-memory-1',
  memory_type: 'agent_memory',
  text: 'Agent memory',
  level: 0,
  status: 'active',
  rollup_status: 'pending',
  created_at: '2026-11-04T00:00:00Z',
  updated_at: '2026-11-04T00:00:00Z',
};

const projectMemory = {
  ...agentMemory,
  id: 'memory-project',
  subject_id: 'project:project-1',
  memory_key: 'project-memory-1',
  memory_type: 'project_summary',
  text: 'Project summary',
};

describe('useAgentExplorer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.listAgents).mockResolvedValue([agent]);
    vi.mocked(api.listAgentProjects).mockResolvedValue([project]);
    vi.mocked(api.listSubjectMemories).mockImplementation(async (subjectId: string) =>
      subjectId.startsWith('project:') ? [projectMemory] : [agentMemory],
    );
    vi.mocked(api.listThreads).mockResolvedValue([thread]);
    vi.mocked(api.listThreadRecords).mockResolvedValue({ items: [], total: 0 });
  });

  it('loads one agent as the entry point with its memory, projects and conversations', async () => {
    const { result } = renderHook(() => useAgentExplorer());

    await waitFor(() => {
      expect(result.current.agentDetailLoading).toBe(false);
      expect(result.current.selectedAgent?.agent_id).toBe('main-chat');
      expect(result.current.projectMemories).toEqual([projectMemory]);
    });

    expect(result.current.agentMemories).toEqual([agentMemory]);
    expect(result.current.projects).toEqual([project]);
    expect(result.current.threadExplorer.threads).toEqual([thread]);
    expect(api.listThreads).toHaveBeenCalledWith(
      expect.objectContaining({
        tenant_id: 'tenant-1',
        source_id: 'chatos',
        agent_id: 'main-chat',
        status: '',
      }),
    );
  });

  it('normalizes directory filters before querying', () => {
    expect(buildAgentFilters({
      tenant_id: ' tenant-2 ',
      source_id: ' chatos ',
      agent_id: ' approval ',
      limit: 25,
    })).toEqual({
      tenant_id: 'tenant-2',
      source_id: 'chatos',
      agent_id: 'approval',
      limit: 25,
      offset: 0,
    });
  });

  it('builds empty filters without leaking whitespace', () => {
    expect(buildAgentFilters({ tenant_id: ' ', source_id: '', agent_id: ' ', limit: 10 }))
      .toEqual({
        tenant_id: undefined,
        source_id: undefined,
        agent_id: undefined,
        limit: 10,
        offset: 0,
      });
  });
});
