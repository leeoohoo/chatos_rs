// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Project } from '../../../types';
import { useProjectRunnerTerminalPolling } from './useProjectRunnerTerminalPolling';

vi.mock('../../../lib/realtime/useProjectRunRealtime', () => ({
  useProjectRunRealtime: () => undefined,
}));

vi.mock('../../../lib/realtime/useTerminalStateRealtime', () => ({
  useTerminalStateRealtime: () => undefined,
}));

const baseProject: Project = {
  id: 'project_1',
  name: 'Project One',
  rootPath: '/workspace/project_1',
  createdAt: new Date('2026-05-20T00:00:00Z'),
  updatedAt: new Date('2026-05-20T00:00:00Z'),
};

const buildRunStateResponse = (projectId: string, terminalId = `${projectId}_terminal`) => ({
  project_id: projectId,
  running: true,
  busy: false,
  status: 'running',
  terminal_id: terminalId,
  terminal_name: `${projectId} terminal`,
  cwd: `/workspace/${projectId}`,
  terminal: {
    id: terminalId,
    name: `${projectId} terminal`,
    cwd: `/workspace/${projectId}`,
    status: 'running',
    busy: false,
    created_at: '2026-05-20T00:00:00Z',
    updated_at: '2026-05-20T00:00:00Z',
    last_active_at: '2026-05-20T00:00:00Z',
  },
  instances: [
    {
      terminal_id: terminalId,
      terminal_name: `${projectId} terminal`,
      cwd: `/workspace/${projectId}`,
      status: 'running',
      busy: false,
      running: true,
      terminal: {
        id: terminalId,
        name: `${projectId} terminal`,
        cwd: `/workspace/${projectId}`,
        status: 'running',
        busy: false,
        created_at: '2026-05-20T00:00:00Z',
        updated_at: '2026-05-20T00:00:00Z',
        last_active_at: '2026-05-20T00:00:00Z',
      },
    },
  ],
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

const TerminalHarness: React.FC<{
  client: {
    getProjectRunState: (projectId: string) => Promise<unknown>;
  };
  project: Project | null;
  enabled?: boolean;
}> = ({ client, project, enabled = true }) => {
  const state = useProjectRunnerTerminalPolling({ client: client as never, project, enabled });

  return (
    <div>
      <div data-testid="instance-count">{state.projectRunInstances.length}</div>
      <div data-testid="selected-instance">{state.selectedRunInstanceId || ''}</div>
      <div data-testid="active-run">{state.activeRun?.terminalId || ''}</div>
      <div data-testid="terminal-busy">{state.activeTerminalBusy ? 'yes' : 'no'}</div>
    </div>
  );
};

const renderHarness = (ui: React.ReactElement) => render(
  ui,
);

describe('useProjectRunnerTerminalPolling', () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('loads active run state when a project becomes active', async () => {
    const client = {
      getProjectRunState: vi.fn(async (projectId: string) => buildRunStateResponse(projectId)),
    };

    renderHarness(<TerminalHarness client={client} project={baseProject} />);

    await waitFor(() => {
      expect(screen.getByTestId('instance-count')).toHaveTextContent('1');
      expect(screen.getByTestId('selected-instance')).toHaveTextContent('project_1_terminal');
    });

    expect(client.getProjectRunState).toHaveBeenCalledTimes(1);
  });

  it('ignores late run-state responses from the previous project after switching', async () => {
    const deferredState = createDeferred<unknown>();
    const client = {
      getProjectRunState: vi.fn((projectId: string) => {
        if (projectId === 'project_1') {
          return deferredState.promise;
        }
        return Promise.resolve(buildRunStateResponse(projectId));
      }),
    };

    const renderResult = renderHarness(<TerminalHarness client={client} project={baseProject} />);

    await waitFor(() => {
      expect(client.getProjectRunState).toHaveBeenCalledWith('project_1');
    });

    const nextProject: Project = {
      ...baseProject,
      id: 'project_2',
      name: 'Project Two',
      rootPath: '/workspace/project_2',
    };

    renderResult.rerender(<TerminalHarness client={client} project={nextProject} />);

    await waitFor(() => {
      expect(screen.getByTestId('selected-instance')).toHaveTextContent('project_2_terminal');
      expect(screen.getByTestId('active-run')).toHaveTextContent('project_2_terminal');
    });

    deferredState.resolve(buildRunStateResponse('project_1', 'project_1_stale_terminal'));

    await waitFor(() => {
      expect(screen.getByTestId('selected-instance')).toHaveTextContent('project_2_terminal');
    });
    expect(screen.getByTestId('active-run')).not.toHaveTextContent('project_1_stale_terminal');
  });
});
