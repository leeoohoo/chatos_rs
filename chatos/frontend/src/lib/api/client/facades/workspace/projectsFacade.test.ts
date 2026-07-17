// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import { workspaceProjectFacade } from './projectsFacade';

describe('workspaceProjectFacade local project management routing', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('merges SQLite local projects with cloud-only projects on desktop', async () => {
    vi.stubGlobal('window', {
      chatosLocalRuntime: { apiRequest: vi.fn() },
    });
    const cloudRequest = vi.fn().mockResolvedValue([
      {
        id: 'legacy-local',
        name: 'Legacy Local',
        execution_plane: 'local_connector',
        root_path: 'local://connector/device/workspace',
      },
      {
        id: 'cloud-1',
        name: 'Cloud',
        execution_plane: 'cloud',
        root_path: 'harness://project/cloud-1',
      },
    ]);
    const listProjects = vi.fn().mockResolvedValue([
      {
        id: 'local-1',
        name: 'Local',
        execution_plane: 'local_connector',
        root_path: 'local://connector/device/workspace/apps',
      },
    ]);
    const context = {
      getRequestFn: () => cloudRequest,
      getLocalRuntimeClient: () => ({ listProjects }),
      registerLocalProjectExecution: vi.fn(),
    };

    const projects = await workspaceProjectFacade.listProjects.call(context as never, 'user-1');

    expect(projects.map((project) => project.id)).toEqual(['local-1', 'cloud-1']);
    expect(listProjects).toHaveBeenCalledTimes(1);
  });

  it('keeps SQLite projects available when the cloud list is offline', async () => {
    vi.stubGlobal('window', {
      chatosLocalRuntime: { apiRequest: vi.fn() },
    });
    const localProject = {
      id: 'local-1',
      name: 'Local',
      execution_plane: 'local_connector',
      root_path: 'local://connector/device/workspace/app',
    };
    const registerLocalProjectExecution = vi.fn();
    const context = {
      getRequestFn: () => vi.fn().mockRejectedValue(new Error('cloud offline')),
      getLocalRuntimeClient: () => ({
        listProjects: vi.fn().mockResolvedValue([localProject]),
      }),
      registerLocalProjectExecution,
    };

    const projects = await workspaceProjectFacade.listProjects.call(context as never, 'user-1');

    expect(projects).toEqual([localProject]);
    expect(registerLocalProjectExecution).toHaveBeenCalledWith('local-1');
  });

  it('does not let a slow cloud list indefinitely block desktop local projects', async () => {
    vi.useFakeTimers();
    vi.stubGlobal('window', {
      chatosLocalRuntime: { apiRequest: vi.fn() },
    });
    const localProject = {
      id: 'local-fast',
      name: 'Local Fast',
      execution_plane: 'local_connector',
      root_path: 'local://connector/device/workspace/app',
    };
    const context = {
      getRequestFn: () => vi.fn().mockReturnValue(new Promise(() => {})),
      getLocalRuntimeClient: () => ({
        listProjects: vi.fn().mockResolvedValue([localProject]),
      }),
      registerLocalProjectExecution: vi.fn(),
    };

    const pending = workspaceProjectFacade.listProjects.call(context as never, 'user-1');
    await vi.advanceTimersByTimeAsync(800);

    await expect(pending).resolves.toEqual([localProject]);
  });

  it('allows browser clients to create cloud projects', async () => {
    vi.stubGlobal('window', {});
    const cloudRequest = vi.fn().mockResolvedValue({
      id: 'cloud-1',
      name: 'Cloud',
      execution_plane: 'cloud',
    });
    const context = {
      getRequestFn: () => cloudRequest,
    };
    const form = new FormData();
    form.set('name', 'Cloud');

    await workspaceProjectFacade.createCloudProject.call(context as never, form);

    expect(cloudRequest).toHaveBeenCalledWith('/projects/cloud', {
      method: 'POST',
      body: form,
    });
  });

  it('keeps local project creation desktop-only', async () => {
    vi.stubGlobal('window', {});
    const context = {
      getRequestFn: () => vi.fn(),
    };

    await expect(workspaceProjectFacade.createProject.call(context as never, {
      name: 'Local',
      root_path: '/tmp/local',
    })).rejects.toThrow('项目只能在 Chat OS 桌面客户端中创建');
  });

  it('routes local plan data to the desktop runtime', async () => {
    const getProjectPlan = vi.fn().mockResolvedValue({ requirements: [] });
    const listProjectRequirementWorkItems = vi.fn().mockResolvedValue({ work_items: [] });
    const listProjectRequirementDocuments = vi.fn().mockResolvedValue([]);
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud request must not run');
    });
    const context = {
      projectUsesLocalRuntime: (projectId: string) => projectId === 'project-local',
      getLocalRuntimeClient: () => ({
        getProjectPlan,
        listProjectRequirementWorkItems,
        listProjectRequirementDocuments,
      }),
      getRequestFn: () => cloudRequest,
    };

    await workspaceProjectFacade.getProjectPlan.call(
      context as never,
      'project-local',
      { includeWorkItems: false },
    );
    await workspaceProjectFacade.listProjectRequirementWorkItems.call(
      context as never,
      'project-local',
      'requirement-local',
      { includeDependencyGraph: true },
    );
    await workspaceProjectFacade.listProjectRequirementDocuments.call(
      context as never,
      'project-local',
      'requirement-local',
    );

    expect(getProjectPlan).toHaveBeenCalledWith(
      'project-local',
      { includeWorkItems: false },
    );
    expect(listProjectRequirementWorkItems).toHaveBeenCalledWith(
      'project-local',
      'requirement-local',
      { includeDependencyGraph: true },
    );
    expect(listProjectRequirementDocuments).toHaveBeenCalledWith(
      'project-local',
      'requirement-local',
    );
    expect(cloudRequest).not.toHaveBeenCalled();
  });

  it('routes local runtime environment operations to the desktop runtime', async () => {
    const getProjectRuntimeEnvironment = vi.fn().mockResolvedValue({ environment: {} });
    const updateProjectRuntimeEnvironmentSettings = vi.fn().mockResolvedValue({ environment: {} });
    const analyzeProjectRuntimeEnvironment = vi.fn().mockResolvedValue({ environment: {} });
    const getProjectRuntimeEnvironmentProgress = vi.fn().mockResolvedValue({ status: 'running' });
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud environment request must not run');
    });
    const context = {
      projectUsesLocalRuntime: (projectId: string) => projectId === 'project-local',
      getLocalRuntimeClient: () => ({
        getProjectRuntimeEnvironment,
        updateProjectRuntimeEnvironmentSettings,
        analyzeProjectRuntimeEnvironment,
        getProjectRuntimeEnvironmentProgress,
      }),
      getRequestFn: () => cloudRequest,
    };

    await workspaceProjectFacade.getProjectRuntimeEnvironment.call(
      context as never,
      'project-local',
    );
    await workspaceProjectFacade.updateProjectRuntimeEnvironmentSettings.call(
      context as never,
      'project-local',
      { sandbox_enabled: true },
    );
    await workspaceProjectFacade.analyzeProjectRuntimeEnvironment.call(
      context as never,
      'project-local',
    );
    await workspaceProjectFacade.getProjectRuntimeEnvironmentProgress.call(
      context as never,
      'project-local',
    );
    await expect(workspaceProjectFacade.generateProjectRuntimeEnvironmentImage.call(
      context as never,
      'project-local',
      'image-1',
    )).rejects.toThrow('本地项目镜像必须由本地客户端生成');

    expect(getProjectRuntimeEnvironment).toHaveBeenCalledWith('project-local');
    expect(updateProjectRuntimeEnvironmentSettings).toHaveBeenCalledWith(
      'project-local',
      { sandbox_enabled: true },
    );
    expect(analyzeProjectRuntimeEnvironment).toHaveBeenCalledWith('project-local');
    expect(getProjectRuntimeEnvironmentProgress).toHaveBeenCalledWith('project-local');
    expect(cloudRequest).not.toHaveBeenCalled();
  });

  it('routes local project run settings to SQLite runtime APIs', async () => {
    const localRuntime = {
      analyzeProjectRun: vi.fn().mockResolvedValue({ targets: [] }),
      getProjectRunCatalog: vi.fn().mockResolvedValue({ targets: [] }),
      getProjectRunState: vi.fn().mockResolvedValue({ status: 'idle' }),
      getProjectRunEnvironment: vi.fn().mockResolvedValue({ env_vars: {} }),
      updateProjectRunEnvironment: vi.fn().mockResolvedValue({ env_vars: { PORT: '3000' } }),
      executeProjectRun: vi.fn().mockResolvedValue({ status: 'running' }),
      setProjectRunDefault: vi.fn().mockResolvedValue({ default_target_id: 'target-1' }),
    };
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud project run request must not run');
    });
    const context = {
      projectUsesLocalRuntime: () => true,
      getLocalRuntimeClient: () => localRuntime,
      getRequestFn: () => cloudRequest,
    };

    await workspaceProjectFacade.analyzeProjectRun.call(context as never, 'local-1');
    await workspaceProjectFacade.getProjectRunCatalog.call(context as never, 'local-1');
    await workspaceProjectFacade.getProjectRunState.call(context as never, 'local-1');
    await workspaceProjectFacade.getProjectRunEnvironment.call(context as never, 'local-1');
    await workspaceProjectFacade.updateProjectRunEnvironment.call(context as never, 'local-1', {
      env_vars: { PORT: '3000' },
    });
    await workspaceProjectFacade.executeProjectRun.call(context as never, 'local-1', {
      target_id: 'target-1',
    });
    await workspaceProjectFacade.setProjectRunDefault.call(context as never, 'local-1', 'target-1');

    Object.values(localRuntime).forEach((method) => expect(method).toHaveBeenCalled());
    expect(cloudRequest).not.toHaveBeenCalled();
  });

  it('routes local requirement execution to the desktop Task Runner', async () => {
    const executeProjectRequirement = vi.fn().mockResolvedValue({ status: 'queued' });
    const stopProjectRequirementExecution = vi.fn().mockResolvedValue({ success: true });
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud Task Runner request must not run');
    });
    const context = {
      projectUsesLocalRuntime: () => true,
      getLocalRuntimeClient: () => ({
        executeProjectRequirement,
        stopProjectRequirementExecution,
      }),
      getRequestFn: () => cloudRequest,
    };

    await workspaceProjectFacade.executeProjectRequirement.call(
      context as never,
      'project-local',
      'requirement-local',
      { include_prerequisite_dependents: true },
    );
    await workspaceProjectFacade.stopProjectRequirementExecution.call(
      context as never,
      'project-local',
      'requirement-local',
      {},
    );

    expect(executeProjectRequirement).toHaveBeenCalledWith(
      'project-local',
      'requirement-local',
      { include_prerequisite_dependents: true },
    );
    expect(stopProjectRequirementExecution).toHaveBeenCalledWith(
      'project-local',
      'requirement-local',
      {},
    );
    expect(cloudRequest).not.toHaveBeenCalled();
  });

  it('marks project contact requests as local without requiring a cloud project record', async () => {
    const request = vi.fn()
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ locked: false })
      .mockResolvedValueOnce({ contact_id: 'contact-1' })
      .mockResolvedValueOnce({ success: true });
    const context = {
      projectUsesLocalRuntime: () => true,
      getRequestFn: () => request,
    };

    await workspaceProjectFacade.listProjectContacts.call(context as never, 'project-local');
    await workspaceProjectFacade.getProjectContactLock.call(context as never, 'project-local');
    await workspaceProjectFacade.addProjectContact.call(
      context as never,
      'project-local',
      { contact_id: 'contact-1' },
    );
    await workspaceProjectFacade.removeProjectContact.call(
      context as never,
      'project-local',
      'contact-1',
    );

    expect(request).toHaveBeenNthCalledWith(
      1,
      '/projects/project-local/contacts?local_runtime=true',
    );
    expect(request).toHaveBeenNthCalledWith(
      2,
      '/projects/project-local/contacts/lock?local_runtime=true',
    );
    expect(request).toHaveBeenNthCalledWith(
      3,
      '/projects/project-local/contacts?local_runtime=true',
      { method: 'POST', body: JSON.stringify({ contact_id: 'contact-1' }) },
    );
    expect(request).toHaveBeenNthCalledWith(
      4,
      '/projects/project-local/contacts/contact-1?local_runtime=true',
      { method: 'DELETE' },
    );
  });
});
