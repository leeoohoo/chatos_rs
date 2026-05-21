// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Project } from '../../../types';
import { useProjectRunnerCatalogState } from './useProjectRunnerCatalogState';

vi.mock('../../../lib/realtime/useProjectRunRealtime', () => ({
  useProjectRunRealtime: () => undefined,
}));

const baseProject: Project = {
  id: 'project_1',
  name: 'Project One',
  rootPath: '/workspace/project_1',
  createdAt: new Date('2026-05-20T00:00:00Z'),
  updatedAt: new Date('2026-05-20T00:00:00Z'),
};

const buildCatalogResponse = (projectId: string, targetId = `${projectId}_target`) => ({
  project_id: projectId,
  default_target_id: targetId,
  targets: [
    {
      id: targetId,
      label: `${projectId} target`,
      kind: 'node',
      cwd: `/workspace/${projectId}`,
      command: 'npm run dev',
      source: 'analyzer',
      confidence: 1,
      required_toolchains: ['node'],
    },
  ],
});

const buildEnvironmentResponse = (projectId: string, envVars: Record<string, string>) => ({
  project_id: projectId,
  options_by_kind: {
    node: [
      {
        id: `node:/opt/${projectId}/node`,
        kind: 'node',
        label: 'node',
        path: `/opt/${projectId}/node`,
        source: 'system',
      },
    ],
  },
  config_files: [],
  validation_issues: [],
  selected_toolchains: {
    node: `node:/opt/${projectId}/node`,
  },
  custom_toolchains: {},
  env_vars: envVars,
});

const createDeferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
};

const CatalogHarness: React.FC<{
  client: {
    getProjectRunCatalog: (projectId: string) => Promise<unknown>;
    analyzeProjectRun: (projectId: string) => Promise<unknown>;
    getProjectRunEnvironment: (projectId: string) => Promise<unknown>;
    setProjectRunDefault: (projectId: string, targetId: string) => Promise<unknown>;
    updateProjectRunEnvironment: (projectId: string, payload: unknown) => Promise<unknown>;
  };
  project: Project | null;
  enabled?: boolean;
}> = ({ client, project, enabled = true }) => {
  const state = useProjectRunnerCatalogState({ client: client as never, project, enabled });

  return (
    <div>
      <div data-testid="status">{state.runStatus}</div>
      <div data-testid="target-count">{state.runTargets.length}</div>
      <div data-testid="selected-target">{state.selectedRunTargetId || ''}</div>
      <div data-testid="env-draft">{state.envVarsDraft}</div>
      <div data-testid="env-error">{state.runEnvironmentError || ''}</div>
      <div data-testid="catalog-loading">{state.runCatalogLoading ? 'yes' : 'no'}</div>
      <div data-testid="environment-loading">{state.runEnvironmentLoading ? 'yes' : 'no'}</div>
    </div>
  );
};

const renderHarness = (ui: React.ReactElement) => render(
  ui,
);

describe('useProjectRunnerCatalogState', () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('loads catalog and environment together when a project becomes active', async () => {
    const client = {
      getProjectRunCatalog: vi.fn(async (projectId: string) => buildCatalogResponse(projectId)),
      analyzeProjectRun: vi.fn(async (projectId: string) => buildCatalogResponse(projectId, `${projectId}_analyze`)),
      getProjectRunEnvironment: vi.fn(async (projectId: string) => buildEnvironmentResponse(projectId, { APP_ENV: 'dev', PORT: '3000' })),
      setProjectRunDefault: vi.fn(async (projectId: string, targetId: string) => buildCatalogResponse(projectId, targetId)),
      updateProjectRunEnvironment: vi.fn(async (projectId: string) => buildEnvironmentResponse(projectId, { APP_ENV: 'dev', PORT: '3000' })),
    };

    renderHarness(<CatalogHarness client={client} project={baseProject} />);

    await waitFor(() => {
      expect(screen.getByTestId('target-count')).toHaveTextContent('1');
    });
    await waitFor(() => {
      expect(screen.getByTestId('env-draft')).toHaveTextContent(/APP_ENV=dev\s+PORT=3000/);
    });

    expect(client.getProjectRunCatalog).toHaveBeenCalledTimes(1);
    expect(client.getProjectRunEnvironment).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId('selected-target')).toHaveTextContent('project_1_target');
  });

  it('ignores late environment responses from the previous project after switching', async () => {
    const deferredEnvironment = createDeferred<unknown>();
    const client = {
      getProjectRunCatalog: vi.fn(async (projectId: string) => buildCatalogResponse(projectId)),
      analyzeProjectRun: vi.fn(async (projectId: string) => buildCatalogResponse(projectId, `${projectId}_analyze`)),
      getProjectRunEnvironment: vi.fn((projectId: string) => {
        if (projectId === 'project_1') {
          return deferredEnvironment.promise;
        }
        return Promise.resolve(buildEnvironmentResponse(projectId, { APP_ENV: 'prod' }));
      }),
      setProjectRunDefault: vi.fn(async (projectId: string, targetId: string) => buildCatalogResponse(projectId, targetId)),
      updateProjectRunEnvironment: vi.fn(async (projectId: string) => buildEnvironmentResponse(projectId, { APP_ENV: 'prod' })),
    };

    const renderResult = renderHarness(<CatalogHarness client={client} project={baseProject} />);

    await waitFor(() => {
      expect(client.getProjectRunCatalog).toHaveBeenCalledWith('project_1');
    });
    await waitFor(() => {
      expect(screen.getByTestId('catalog-loading')).toHaveTextContent('no');
    });

    const nextProject: Project = {
      ...baseProject,
      id: 'project_2',
      name: 'Project Two',
      rootPath: '/workspace/project_2',
    };
    renderResult.rerender(<CatalogHarness client={client} project={nextProject} />);

    await waitFor(() => {
      expect(screen.getByTestId('target-count')).toHaveTextContent('1');
      expect(screen.getByTestId('selected-target')).toHaveTextContent('project_2_target');
    });
    await waitFor(() => {
      expect(screen.getByTestId('env-draft')).toHaveTextContent('APP_ENV=prod');
    });

    deferredEnvironment.resolve(buildEnvironmentResponse('project_1', { APP_ENV: 'stale' }));

    await waitFor(() => {
      expect(screen.getByTestId('env-draft')).toHaveTextContent('APP_ENV=prod');
    });
    expect(screen.getByTestId('env-draft')).not.toHaveTextContent('APP_ENV=stale');
  });
});
