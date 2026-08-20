// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import { workspaceProjectFacade } from './projectsFacade';

describe('workspaceProjectFacade server orchestration', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('uses the project service catalog as the only business source', async () => {
    const projects = [
      { id: 'local-root-project', root_path: 'local://connector/device/workspace/app' },
    ];
    const request = vi.fn().mockResolvedValue(projects);

    await expect(workspaceProjectFacade.listProjects.call(
      { getRequestFn: () => request } as never,
      'user-1',
    )).resolves.toEqual(projects);
    expect(request).toHaveBeenCalledWith('/projects?user_id=user-1');
  });

  it('surfaces project service failures instead of using stale SQLite data', async () => {
    const request = vi.fn().mockRejectedValue(new Error('project service offline'));
    await expect(workspaceProjectFacade.listProjects.call(
      { getRequestFn: () => request } as never,
      'user-1',
    )).rejects.toThrow('project service offline');
  });

  it('routes project planning through the project API', async () => {
    const request = vi.fn().mockResolvedValue({});
    const context = { getRequestFn: () => request };

    await workspaceProjectFacade.getProjectPlan.call(
      context as never,
      'project-1',
      { includeWorkItems: false },
    );
    expect(request).toHaveBeenCalledWith(
      '/projects/project-1/plan?include_work_items=false',
    );
  });

  it('routes requirement execution and project runs through server APIs', async () => {
    const request = vi.fn().mockResolvedValue({});
    const context = { getRequestFn: () => request };

    await workspaceProjectFacade.executeProjectRequirement.call(
      context as never,
      'project-1',
      'requirement-1',
      { planning_feedback: '先补测试' },
    );
    await workspaceProjectFacade.executeProjectRun.call(
      context as never,
      'project-1',
      { target_id: 'target-1' },
    );

    expect(request).toHaveBeenNthCalledWith(
      1,
      '/projects/project-1/requirements/requirement-1/execute',
      { method: 'POST', body: JSON.stringify({ planning_feedback: '先补测试' }) },
    );
    expect(request).toHaveBeenNthCalledWith(
      2,
      '/projects/project-1/run/execute',
      { method: 'POST', body: JSON.stringify({ target_id: 'target-1' }) },
    );
  });

  it('uses ordinary contact routes without local-runtime flags', async () => {
    const request = vi.fn().mockResolvedValue({});
    const context = { getRequestFn: () => request };

    await workspaceProjectFacade.listProjectContacts.call(context as never, 'project-1');
    await workspaceProjectFacade.getProjectContactLock.call(context as never, 'project-1');
    await workspaceProjectFacade.addProjectContact.call(
      context as never,
      'project-1',
      { contact_id: 'contact-1' },
    );
    await workspaceProjectFacade.removeProjectContact.call(
      context as never,
      'project-1',
      'contact-1',
    );

    expect(request).toHaveBeenNthCalledWith(1, '/projects/project-1/contacts');
    expect(request).toHaveBeenNthCalledWith(2, '/projects/project-1/contacts/lock');
    expect(request).toHaveBeenNthCalledWith(3, '/projects/project-1/contacts', {
      method: 'POST',
      body: JSON.stringify({ contact_id: 'contact-1' }),
    });
    expect(request).toHaveBeenNthCalledWith(4, '/projects/project-1/contacts/contact-1', {
      method: 'DELETE',
    });
  });
});
