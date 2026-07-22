// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import type {
  ChatStoreDraft,
  ChatStoreShape,
} from '../types';
import { createProjectActions } from './projects';

describe('loadProjects', () => {
  it('lets a forced refresh supersede an older in-flight project request', async () => {
    const state = {
      projects: [],
      currentProjectId: null,
      currentProject: null,
      activePanel: 'chat',
      error: null,
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    let resolveInitial: ((value: unknown[]) => void) | undefined;
    const listProjects = vi.fn()
      .mockImplementationOnce(() => new Promise<unknown[]>((resolve) => {
        resolveInitial = resolve;
      }))
      .mockResolvedValueOnce([{
        id: 'project_fresh',
        name: 'Fresh Project',
        source_type: 'cloud',
        execution_plane: 'cloud',
        root_path: 'harness://project/project_fresh',
      }]);
    const actions = createProjectActions({
      set,
      get: () => state,
      client: {
        registerProjectExecution: vi.fn(),
        listProjects,
      } as never,
      getUserIdParam: () => 'user_force_projects',
    });

    const initial = actions.loadProjects();
    await Promise.resolve();
    await actions.loadProjects({ force: true });
    resolveInitial?.([{
      id: 'project_stale',
      name: 'Stale Project',
      source_type: 'cloud',
      execution_plane: 'cloud',
      root_path: 'harness://project/project_stale',
    }]);
    await initial;

    expect(listProjects).toHaveBeenCalledTimes(2);
    expect(state.projects.map((project) => project.id)).toEqual(['project_fresh']);
  });

  it('clears stale current project state when the selected project no longer exists', async () => {
    const state = {
      projects: [],
      currentProjectId: 'project_missing',
      currentProject: {
        id: 'project_missing',
        name: 'Missing Project',
        rootPath: '/tmp/missing-project',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      },
      activePanel: 'project',
      error: null,
    } as unknown as ChatStoreShape;

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    const actions = createProjectActions({
      set,
      get,
      client: {
        registerProjectExecution: vi.fn(),
        listProjects: vi.fn().mockResolvedValue([
          {
            id: 'project_kept',
            name: 'Kept Project',
            root_path: '/tmp/kept-project',
            git_url: 'git@github.com:org/kept.git',
            created_at: '2026-01-01T00:00:00.000Z',
            updated_at: '2026-01-01T00:00:00.000Z',
          },
        ]),
      } as never,
      getUserIdParam: () => 'user_1',
    });

    await actions.loadProjects({ force: true });

    expect(state.currentProjectId).toBeNull();
    expect(state.currentProject).toBeNull();
    expect(state.activePanel).toBe('chat');
    expect(state.projects.map((project) => project.id)).toEqual(['project_kept']);
    expect(state.projects[0]?.gitUrl).toBe('git@github.com:org/kept.git');
  });

  it('filters local projects from the browser project list', async () => {
    const state = {
      projects: [],
      currentProjectId: 'cloud-project',
      currentProject: null,
      activePanel: 'chat',
      error: null,
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const actions = createProjectActions({
      set,
      get: () => state,
      client: {
        registerProjectExecution: vi.fn(),
        listProjects: vi.fn().mockResolvedValue([
          {
            id: 'cloud-project',
            name: 'Cloud',
            source_type: 'cloud',
            execution_plane: 'cloud',
            root_path: 'harness://project/cloud-project',
          },
          {
            id: 'local-project',
            name: 'Local',
            source_type: 'local_connector',
            execution_plane: 'local_connector',
            root_path: 'local://connector/device/workspace',
          },
        ]),
      } as never,
      getUserIdParam: () => 'user_1',
    });

    await actions.loadProjects({ force: true });

    expect(state.projects.map((project) => project.id)).toEqual(['cloud-project']);
  });
});

describe('updateProject', () => {
  it('sends an explicit empty git url so the backend can clear it', async () => {
    const state = {
      projects: [{
        id: 'project_1',
        name: 'Project 1',
        rootPath: '/tmp/project-1',
        gitUrl: 'git@github.com:org/project-1.git',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      }],
      currentProjectId: 'project_1',
      currentProject: {
        id: 'project_1',
        name: 'Project 1',
        rootPath: '/tmp/project-1',
        gitUrl: 'git@github.com:org/project-1.git',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      },
      error: null,
    } as unknown as ChatStoreShape;

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;
    const updateProject = vi.fn().mockResolvedValue({
      id: 'project_1',
      name: 'Project 1',
      root_path: '/tmp/project-1',
      git_url: null,
      created_at: '2026-01-01T00:00:00.000Z',
      updated_at: '2026-01-02T00:00:00.000Z',
    });

    const actions = createProjectActions({
      set,
      get,
      client: {
        registerProjectExecution: vi.fn(),
        updateProject,
      } as never,
      getUserIdParam: () => 'user_1',
    });

    const project = await actions.updateProject('project_1', { gitUrl: '' });

    expect(updateProject).toHaveBeenCalledWith('project_1', { git_url: '' });
    expect(project?.gitUrl).toBeNull();
    expect(state.currentProject?.gitUrl).toBeNull();
  });
});
