// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import { workspaceProjectFacade } from './projectsFacade';

describe('workspaceProjectFacade local project management routing', () => {
  afterEach(() => {
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
    };

    const projects = await workspaceProjectFacade.listProjects.call(context as never, 'user-1');

    expect(projects.map((project) => project.id)).toEqual(['local-1', 'cloud-1']);
    expect(listProjects).toHaveBeenCalledTimes(1);
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
});
