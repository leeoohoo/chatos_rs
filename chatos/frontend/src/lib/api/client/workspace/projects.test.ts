// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import {
  getProjectContactLock,
  getProjectPlan,
  getProjectRequirementExecutionPlan,
  listProjectRequirementDocuments,
  listProjectRequirementWorkItems,
  pauseProjectRequirementExecution,
  rerunProjectRequirementExecution,
  resumeProjectRequirementExecution,
  stopProjectRequirementExecution,
} from './projects';

describe('workspace project api helpers', () => {
  it('loads the project contact lock state from the Chatos project endpoint', async () => {
    const request = vi.fn().mockResolvedValue({ locked: false });

    await getProjectContactLock(request as never, 'project with spaces');

    expect(request).toHaveBeenCalledTimes(1);
    expect(request).toHaveBeenCalledWith('/projects/project%20with%20spaces/contacts/lock');
  });

  it('passes lightweight plan options as query parameters', async () => {
    const request = vi.fn().mockResolvedValue({});

    await getProjectPlan(request as never, 'project 1', { includeWorkItems: false });

    expect(request).toHaveBeenCalledWith('/projects/project%201/plan?include_work_items=false');
  });

  it('loads requirement work items through the project-scoped endpoint', async () => {
    const request = vi.fn().mockResolvedValue({});

    await listProjectRequirementWorkItems(request as never, 'project 1', 'req/1', {
      includeDependencyGraph: true,
    });

    expect(request).toHaveBeenCalledWith(
      '/projects/project%201/requirements/req%2F1/work-items?include_dependency_graph=true',
    );
  });

  it('loads requirement documents through the project-scoped endpoint', async () => {
    const request = vi.fn().mockResolvedValue([]);

    await listProjectRequirementDocuments(request as never, 'project 1', 'req/1');

    expect(request).toHaveBeenCalledWith(
      '/projects/project%201/requirements/req%2F1/documents',
    );
  });

  it('reads a precise persisted execution plan identity for reopening', async () => {
    const request = vi.fn().mockResolvedValue({ found: true });

    await getProjectRequirementExecutionPlan(request as never, 'project 1', 'req/1', {
      conversationId: 'session-1',
      executionGroupId: 'execution-group-1',
    });

    expect(request).toHaveBeenCalledWith(
      '/projects/project%201/requirements/req%2F1/execution-plan?conversation_id=session-1&execution_group_id=execution-group-1',
    );
  });

  it('sends the exact plan identity when abandoning a generated task graph', async () => {
    const request = vi.fn().mockResolvedValue({ success: true });

    await stopProjectRequirementExecution(request as never, 'project 1', 'req/1', {
      contact_id: 'contact-1',
      execution_group_id: 'execution-group-1',
      conversation_id: 'session-1',
      discard_tasks: true,
    });

    expect(request).toHaveBeenCalledWith(
      '/projects/project%201/requirements/req%2F1/stop',
      {
        method: 'POST',
        body: JSON.stringify({
          contact_id: 'contact-1',
          execution_group_id: 'execution-group-1',
          conversation_id: 'session-1',
          discard_tasks: true,
        }),
      },
    );
  });

  it('sends the exact stopped batch identity when rerunning a requirement execution', async () => {
    const request = vi.fn().mockResolvedValue({ success: true });

    await rerunProjectRequirementExecution(request as never, 'project 1', 'req/1', {
      contact_id: 'contact-1',
      execution_group_id: 'execution-group-1',
      conversation_id: 'session-1',
    });

    expect(request).toHaveBeenCalledWith(
      '/projects/project%201/requirements/req%2F1/rerun',
      {
        method: 'POST',
        body: JSON.stringify({
          contact_id: 'contact-1',
          execution_group_id: 'execution-group-1',
          conversation_id: 'session-1',
        }),
      },
    );
  });

  it('uses distinct pause and resume endpoints for a confirmed execution batch', async () => {
    const request = vi.fn().mockResolvedValue({ success: true });
    const payload = {
      contact_id: 'contact-1',
      execution_group_id: 'execution-group-1',
      conversation_id: 'session-1',
    };

    await pauseProjectRequirementExecution(
      request as never,
      'project 1',
      'req/1',
      payload,
    );
    await resumeProjectRequirementExecution(
      request as never,
      'project 1',
      'req/1',
      payload,
    );

    expect(request).toHaveBeenNthCalledWith(
      1,
      '/projects/project%201/requirements/req%2F1/pause',
      { method: 'POST', body: JSON.stringify(payload) },
    );
    expect(request).toHaveBeenNthCalledWith(
      2,
      '/projects/project%201/requirements/req%2F1/resume',
      { method: 'POST', body: JSON.stringify(payload) },
    );
  });
});
