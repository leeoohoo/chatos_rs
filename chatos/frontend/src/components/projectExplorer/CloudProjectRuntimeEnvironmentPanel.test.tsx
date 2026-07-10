// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import { ApiClientProvider } from '../../lib/api/ApiClientContext';
import type ApiClient from '../../lib/api/client';
import CloudProjectRuntimeEnvironmentPanel from './CloudProjectRuntimeEnvironmentPanel';

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

describe('CloudProjectRuntimeEnvironmentPanel', () => {
  it('renders the Harness-backed cloud runtime environment and analyzes it', async () => {
    const response = {
      environment: {
        project_id: 'project-1',
        status: 'ready',
        sandbox_enabled: true,
        sandbox_provider: 'cloud_sandbox_manager',
        file_provider: 'harness',
        analysis_summary: 'Python and Bun runtime detected.',
        detected_stack: { languages: ['Python', 'TypeScript'] },
        required_services: [],
        env_vars: { PYTHONPATH: '.' },
        last_agent_run_id: 'agent-run-1',
        updated_at: '2026-07-10T10:00:00Z',
      },
      images: [{
        id: 'image-1',
        environment_key: 'runtime',
        display_name: 'Cloud runtime',
        image_provider: 'cloud_sandbox_manager',
        image_ref: 'runtime:latest',
        status: 'ready',
        ports: [],
        env_vars: { PYTHONPATH: '.' },
      }],
    };
    const getProjectRuntimeEnvironment = vi.fn(async () => response);
    const analyzeProjectRuntimeEnvironment = vi.fn(async () => response);
    const client = {
      getProjectRuntimeEnvironment,
      analyzeProjectRuntimeEnvironment,
    } as unknown as ApiClient;

    render(
      <ApiClientProvider client={client}>
        <I18nProvider>
          <CloudProjectRuntimeEnvironmentPanel
            projectId="project-1"
            projectName="AI Job Search"
            projectSourceType="cloud"
          />
        </I18nProvider>
      </ApiClientProvider>,
    );

    expect(await screen.findByText('Python and Bun runtime detected.')).toBeInTheDocument();
    expect(screen.getAllByText('已就绪').length).toBeGreaterThan(0);
    expect(screen.getByText('harness')).toBeInTheDocument();
    expect(screen.getByText('PYTHONPATH')).toBeInTheDocument();
    expect(screen.getByText('runtime:latest')).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: '固定使用沙箱' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: '固定使用沙箱' })).toBeDisabled();

    fireEvent.click(screen.getByRole('button', { name: '初始化/重新分析' }));
    await waitFor(() => {
      expect(analyzeProjectRuntimeEnvironment).toHaveBeenCalledWith('project-1');
    });
    expect(getProjectRuntimeEnvironment).toHaveBeenCalledWith('project-1');
  });

  it('uses the local project boundary when rendering a local sandbox runtime', async () => {
    const response = {
      environment: {
        project_id: 'local-project-1',
        status: 'ready',
        sandbox_enabled: true,
        sandbox_provider: 'local_connector',
        file_provider: 'local_connector',
      },
      images: [],
    };
    const client = {
      getProjectRuntimeEnvironment: vi.fn(async () => response),
      analyzeProjectRuntimeEnvironment: vi.fn(async () => response),
    } as unknown as ApiClient;

    render(
      <ApiClientProvider client={client}>
        <I18nProvider>
          <CloudProjectRuntimeEnvironmentPanel
            projectId="local-project-1"
            projectName="Local Project"
            projectSourceType="local"
          />
        </I18nProvider>
      </ApiClientProvider>,
    );

    expect((await screen.findAllByText('local_connector')).length).toBeGreaterThanOrEqual(2);
    expect(screen.getByRole('checkbox', { name: '已启用沙箱' })).toBeChecked();
    expect(screen.getByText('沙箱运行环境')).toBeInTheDocument();
  });

  it('disables initialization while the backend is analyzing and surfaces failed build logs', async () => {
    const analyzingResponse = {
      environment: {
        project_id: 'project-2',
        status: 'analyzing',
        sandbox_enabled: true,
        sandbox_provider: 'local_connector',
        file_provider: 'harness',
        last_agent_run_id: 'agent-run-2',
        updated_at: '2026-07-10T10:00:00Z',
      },
      images: [],
    };
    const failedResponse = {
      ...analyzingResponse,
      environment: {
        ...analyzingResponse.environment,
        status: 'failed',
        last_error: 'docker build failed: externally-managed-environment',
      },
    };
    const getProjectRuntimeEnvironment = vi
      .fn()
      .mockResolvedValueOnce(analyzingResponse)
      .mockResolvedValue(failedResponse);
    const progressResponse = {
      project_id: 'project-2',
      run_id: 'agent-run-2',
      phase: 'failed',
      status: 'failed',
      progress_percent: 100,
      provider: 'local_connector',
      job_id: 'image-job-2',
      image_id: 'local-node-24-python-3-12',
      started_at: '2026-07-10T10:00:01Z',
      updated_at: '2026-07-10T10:01:00Z',
      logs: 'error: externally-managed-environment',
      error: 'docker build failed',
    };
    let resolveProgress: ((value: typeof progressResponse) => void) | undefined;
    const getProjectRuntimeEnvironmentProgress = vi.fn(() => new Promise<typeof progressResponse>((resolve) => {
      resolveProgress = resolve;
    }));
    const client = {
      getProjectRuntimeEnvironment,
      getProjectRuntimeEnvironmentProgress,
      analyzeProjectRuntimeEnvironment: vi.fn(async () => analyzingResponse),
    } as unknown as ApiClient;

    render(
      <ApiClientProvider client={client}>
        <I18nProvider>
          <CloudProjectRuntimeEnvironmentPanel
            projectId="project-2"
            projectName="AI Job Search"
            projectSourceType="cloud"
          />
        </I18nProvider>
      </ApiClientProvider>,
    );

    expect(await screen.findByRole('button', { name: '分析中...' })).toBeDisabled();
    await act(async () => {
      resolveProgress?.(progressResponse);
    });
    expect(await screen.findByText('error: externally-managed-environment')).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '初始化/重新分析' })).toBeEnabled();
    });
    expect(getProjectRuntimeEnvironmentProgress).toHaveBeenCalledWith('project-2');
  });
});
