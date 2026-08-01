// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import { workspaceProjectFacade } from './projectsFacade';

describe('workspaceProjectFacade cloud orchestration', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('uses the cloud project catalog as the only business source', async () => {
    const projects = [
      { id: 'local-root-project', root_path: 'local://connector/device/workspace/app' },
      { id: 'cloud-project', root_path: 'harness://project/cloud-project' },
    ];
    const request = vi.fn().mockResolvedValue(projects);

    await expect(workspaceProjectFacade.listProjects.call(
      { getRequestFn: () => request } as never,
      'user-1',
    )).resolves.toEqual(projects);
    expect(request).toHaveBeenCalledWith('/projects?user_id=user-1');
  });

  it('surfaces cloud project catalog failures instead of using stale SQLite data', async () => {
    const request = vi.fn().mockRejectedValue(new Error('cloud offline'));
    await expect(workspaceProjectFacade.listProjects.call(
      { getRequestFn: () => request } as never,
      'user-1',
    )).rejects.toThrow('cloud offline');
  });

  it('allows browser clients to create cloud projects', async () => {
    vi.stubGlobal('window', {});
    const request = vi.fn().mockResolvedValue({ id: 'cloud-1' });
    const form = new FormData();
    form.set('name', 'Cloud');

    await workspaceProjectFacade.createCloudProject.call(
      { getRequestFn: () => request } as never,
      form,
    );

    expect(request).toHaveBeenCalledWith('/projects/cloud', { method: 'POST', body: form });
  });

  it('keeps direct local workspace project creation desktop-only', async () => {
    vi.stubGlobal('window', {});
    await expect(workspaceProjectFacade.createProject.call(
      { getRequestFn: () => vi.fn() } as never,
      { name: 'Local', root_path: '/tmp/local' },
    )).rejects.toThrow('项目只能在 Chat OS 桌面客户端中创建');
  });

  it('routes project planning and environment operations through cloud APIs', async () => {
    const request = vi.fn().mockResolvedValue({});
    const context = { getRequestFn: () => request };

    await workspaceProjectFacade.getProjectPlan.call(
      context as never,
      'project-1',
      { includeWorkItems: false },
    );
    await workspaceProjectFacade.analyzeProjectRuntimeEnvironment.call(
      context as never,
      'project-1',
      { analysis_requirement: 'Use Node.js 22' },
    );
    await workspaceProjectFacade.generateProjectRuntimeEnvironmentImage.call(
      context as never,
      'project-1',
      'image-1',
    );

    expect(request).toHaveBeenNthCalledWith(
      1,
      '/projects/project-1/plan?include_work_items=false',
    );
    expect(request).toHaveBeenNthCalledWith(
      2,
      '/projects/project-1/runtime-environment/analyze',
      { method: 'POST', body: JSON.stringify({ analysis_requirement: 'Use Node.js 22' }) },
    );
    expect(request).toHaveBeenNthCalledWith(
      3,
      '/projects/project-1/runtime-environment/images/image-1/generate',
      { method: 'POST' },
    );
  });

  it('routes requirement execution and project runs through cloud APIs', async () => {
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

  it('uses ordinary cloud contact routes without local-runtime flags', async () => {
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
