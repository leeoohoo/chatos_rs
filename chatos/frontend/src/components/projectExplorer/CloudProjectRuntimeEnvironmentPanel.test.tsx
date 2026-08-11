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
        environment_key: 'workspace',
        environment_type: 'workspace',
        display_name: 'Project Workspace',
        service_id: 'workspace',
        service_role: 'workspace',
        mcp_policy: {
          managed_by: 'system',
          attachment: 'workspace_gateway_target',
          filesystem: true,
          terminal: true,
        },
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
    expect(screen.getByText('云端项目')).toBeInTheDocument();
    expect(screen.getByText('源码与运行环境都由云端侧统一编排。')).toBeInTheDocument();
    expect(screen.getByText('文件读取来自云端仓库；镜像准备与运行状态由云端统一维护。')).toBeInTheDocument();
    expect(screen.getAllByText('已就绪').length).toBeGreaterThan(0);
    expect(screen.getByText('harness')).toBeInTheDocument();
    expect(screen.getByText('PYTHONPATH')).toBeInTheDocument();
    expect(screen.getByText('runtime:latest')).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: '固定使用沙箱' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: '固定使用沙箱' })).toBeDisabled();

    fireEvent.click(screen.getByRole('button', { name: '初始化/重新分析' }));
    expect(screen.getByRole('dialog', { name: '运行环境分析要求' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('checkbox', { name: 'PostgreSQL' }));
    fireEvent.click(screen.getByRole('checkbox', { name: 'Redis' }));
    fireEvent.change(screen.getByLabelText('其他分析要求（可选）'), {
      target: { value: '使用 Node.js 22，并将服务暴露在 3000 端口。' },
    });
    fireEvent.click(screen.getByRole('button', { name: '开始分析' }));
    await waitFor(() => {
      expect(analyzeProjectRuntimeEnvironment).toHaveBeenCalledWith('project-1', {
        analysis_requirement: '使用 Node.js 22，并将服务暴露在 3000 端口。',
        prefer_china_mirrors: false,
        selected_dependencies: ['PostgreSQL', 'Redis'],
      });
    });
    expect(getProjectRuntimeEnvironment).toHaveBeenCalledWith('project-1');
  });

  it('submits selected dependencies without requiring custom text and supports common selection', async () => {
    const response = {
      environment: {
        project_id: 'project-dependencies',
        status: 'ready',
        sandbox_enabled: true,
        sandbox_provider: 'cloud_sandbox_manager',
        file_provider: 'harness',
      },
      images: [],
    };
    const analyzeProjectRuntimeEnvironment = vi.fn(async () => response);
    const client = {
      getProjectRuntimeEnvironment: vi.fn(async () => response),
      analyzeProjectRuntimeEnvironment,
    } as unknown as ApiClient;

    render(
      <ApiClientProvider client={client}>
        <I18nProvider>
          <CloudProjectRuntimeEnvironmentPanel
            projectId="project-dependencies"
            projectName="Dependency project"
            projectSourceType="cloud"
          />
        </I18nProvider>
      </ApiClientProvider>,
    );

    fireEvent.click(await screen.findByRole('button', { name: '初始化/重新分析' }));
    const startButton = screen.getByRole('button', { name: '开始分析' });
    expect(startButton).toBeDisabled();

    fireEvent.click(screen.getByRole('button', { name: '选择常用组合' }));
    expect(screen.getByRole('checkbox', { name: 'PostgreSQL' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Redis' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'RabbitMQ' })).toBeChecked();
    expect(screen.getByText('已选择 6 项')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '清空' }));
    expect(screen.getByRole('checkbox', { name: 'PostgreSQL' })).not.toBeChecked();
    expect(startButton).toBeDisabled();

    fireEvent.click(screen.getByRole('checkbox', { name: 'PostgreSQL' }));
    fireEvent.click(screen.getByRole('checkbox', { name: 'Redis' }));
    expect(startButton).toBeEnabled();
    fireEvent.click(startButton);

    await waitFor(() => {
      expect(analyzeProjectRuntimeEnvironment).toHaveBeenCalledWith('project-dependencies', {
        analysis_requirement: undefined,
        prefer_china_mirrors: false,
        selected_dependencies: ['PostgreSQL', 'Redis'],
      });
    });
  });

  it('appends the China mirror requirement when requested during analysis', async () => {
    const response = {
      environment: {
        project_id: 'project-china-mirror',
        status: 'ready',
        sandbox_enabled: true,
        sandbox_provider: 'cloud_sandbox_manager',
        file_provider: 'harness',
      },
      images: [],
    };
    const analyzeProjectRuntimeEnvironment = vi.fn(async () => response);
    const client = {
      getProjectRuntimeEnvironment: vi.fn(async () => response),
      analyzeProjectRuntimeEnvironment,
    } as unknown as ApiClient;

    render(
      <ApiClientProvider client={client}>
        <I18nProvider>
          <CloudProjectRuntimeEnvironmentPanel
            projectId="project-china-mirror"
            projectName="China mirror project"
            projectSourceType="cloud"
          />
        </I18nProvider>
      </ApiClientProvider>,
    );

    fireEvent.click(await screen.findByRole('button', { name: '初始化/重新分析' }));
    fireEvent.click(screen.getByRole('checkbox', { name: /优先生成国内镜像源 Dockerfile/ }));
    fireEvent.click(screen.getByRole('button', { name: '开始分析' }));

    await waitFor(() => {
      expect(analyzeProjectRuntimeEnvironment).toHaveBeenCalledWith('project-china-mirror', {
        analysis_requirement: undefined,
        prefer_china_mirrors: true,
        selected_dependencies: [],
      });
    });
  });

  it('renders detected stack and dependency service config as readable summaries instead of raw JSON', async () => {
    const response = {
      environment: {
        project_id: 'project-readable-runtime',
        status: 'ready',
        sandbox_enabled: true,
        sandbox_provider: 'cloud_sandbox_manager',
        file_provider: 'harness',
        detected_stack: {
          reference_files: [
            'mdm-service/src/mdm_service/server.py',
            'mdm-service/sql/postgresql.sql',
          ],
          reference_count: 10,
          summary: 'Repository contains one independently runnable Python HTTP service under mdm-service. mdm-service uses Python >=3.11 and optional PostgreSQL. Root HTML/CSS/JS pages are a static prototype artifact.',
        },
        required_services: [{
          config: {
            environment_key: 'postgresql',
            type: 'postgres',
            version: '16',
            required: true,
            ports: [{ container_port: 5432, protocol: 'tcp' }],
            database: 'mdm_service',
            username: 'mdm_service',
          },
        }],
      },
      images: [],
    };
    const client = {
      getProjectRuntimeEnvironment: vi.fn(async () => response),
      analyzeProjectRuntimeEnvironment: vi.fn(async () => response),
    } as unknown as ApiClient;

    const { container } = render(
      <ApiClientProvider client={client}>
        <I18nProvider>
          <CloudProjectRuntimeEnvironmentPanel
            projectId="project-readable-runtime"
            projectName="Readable runtime"
            projectSourceType="cloud"
          />
        </I18nProvider>
      </ApiClientProvider>,
    );

    expect(await screen.findByText('技术摘要')).toBeInTheDocument();
    expect(screen.getAllByRole('listitem')).toHaveLength(3);
    expect(screen.getByText('Repository contains one independently runnable Python HTTP service under mdm-service.')).toBeInTheDocument();
    expect(screen.getByText('mdm-service uses Python >=3.11 and optional PostgreSQL.')).toBeInTheDocument();
    expect(screen.getByText('Root HTML/CSS/JS pages are a static prototype artifact.')).toBeInTheDocument();
    expect(screen.getByText('证据文件 (2)')).toBeInTheDocument();
    expect(screen.getByText('mdm-service/src/mdm_service/server.py')).toBeInTheDocument();
    expect(screen.getByText('10')).toBeInTheDocument();
    expect(screen.getAllByText('postgresql').length).toBeGreaterThan(0);
    expect(screen.getByText('16')).toBeInTheDocument();
    expect(screen.getByText('5432/tcp')).toBeInTheDocument();
    expect(screen.getAllByText('mdm_service').length).toBeGreaterThan(0);
    expect(container.textContent).not.toContain('"reference_files"');
    expect(container.textContent).not.toContain('"container_port"');
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
      images: [
        {
          id: 'local-workspace',
          environment_key: 'workspace',
          environment_type: 'workspace',
          display_name: 'Project Workspace',
          service_id: 'workspace',
          service_role: 'workspace',
          mcp_policy: {
            managed_by: 'system',
            attachment: 'workspace_gateway_target',
            filesystem: true,
            terminal: true,
          },
          image_provider: 'local_connector',
          status: 'local',
          ports: [],
          env_vars: {},
        },
        {
          id: 'local-image-1',
          environment_key: 'app',
          environment_type: 'application',
          display_name: 'Local application',
          service_id: 'app',
          service_role: 'application',
          mcp_policy: {
            managed_by: 'system',
            attachment: 'none',
            filesystem: false,
            terminal: false,
          },
          image_provider: 'local_connector',
          dockerfile: 'FROM node:22\nCMD ["npm", "start"]',
          status: 'planned',
          ports: [3000],
          env_vars: {},
        },
      ],
    };
    const pendingResponse = {
      ...response,
      environment: { ...response.environment, status: 'pending_image_build' },
      images: response.images.map((image) => ({ ...image, status: 'building' })),
    };
    const readyResponse = {
      ...response,
      images: response.images.map((image) => ({
        ...image,
        image_ref: 'chatos-local-project-app',
        status: 'running',
      })),
    };
    const generateProjectRuntimeEnvironmentImage = vi.fn(async () => pendingResponse);
    const client = {
      getProjectRuntimeEnvironment: vi.fn()
        .mockResolvedValueOnce(response)
        .mockResolvedValue(readyResponse),
      getProjectRuntimeEnvironmentProgress: vi.fn(async () => ({ status: 'succeeded' })),
      analyzeProjectRuntimeEnvironment: vi.fn(async () => response),
      generateProjectRuntimeEnvironmentImage,
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
    expect(screen.getByText('本地项目')).toBeInTheDocument();
    expect(screen.getByText('云端编排运行环境，本机能力通过 Local Connector 和网关受控暴露。')).toBeInTheDocument();
    expect(screen.getByText('当前项目不会绕过网关访问本机目录；云端只会通过 Local Connector 网关读取文件、启动本地沙箱并同步状态。')).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: '已启用沙箱' })).toBeChecked();
    expect(screen.getByText('本地沙箱构建计划')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '构建并启动本地环境' }));
    await waitFor(() => {
      expect(generateProjectRuntimeEnvironmentImage).toHaveBeenCalledWith(
        'local-project-1',
        'local-workspace',
      );
    });
    expect(await screen.findByRole('button', { name: '本地环境已启动' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: '查看 Dockerfile' }));
    expect(screen.getByText('FROM node:22', { exact: false })).toBeInTheDocument();
  });

  it('shows actionable feedback when the environment initialization model is missing', async () => {
    const pendingConfigurationResponse = {
      environment: {
        project_id: 'project-config',
        status: 'pending_configuration',
        sandbox_enabled: true,
        sandbox_provider: 'cloud_sandbox_manager',
        file_provider: 'harness',
        analysis_summary: '缺少环境初始化模型。',
      },
      images: [],
    };
    const analyzeProjectRuntimeEnvironment = vi.fn(async () => pendingConfigurationResponse);
    const client = {
      getProjectRuntimeEnvironment: vi.fn(async () => pendingConfigurationResponse),
      analyzeProjectRuntimeEnvironment,
    } as unknown as ApiClient;

    render(
      <ApiClientProvider client={client}>
        <I18nProvider>
          <CloudProjectRuntimeEnvironmentPanel
            projectId="project-config"
            projectName="Missing model project"
            projectSourceType="cloud"
          />
        </I18nProvider>
      </ApiClientProvider>,
    );

    expect(await screen.findByRole('alert')).toHaveTextContent('缺少环境初始化模型。');
    expect(screen.getByRole('button', { name: '暂无可准备镜像' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: '检查配置并初始化' }));
    fireEvent.change(screen.getByLabelText('其他分析要求（可选）'), {
      target: { value: '检查现有配置，并继续使用项目声明的运行时版本。' },
    });
    fireEvent.click(screen.getByRole('button', { name: '开始分析' }));

    await waitFor(() => {
      expect(analyzeProjectRuntimeEnvironment).toHaveBeenCalledWith('project-config', {
        analysis_requirement: '检查现有配置，并继续使用项目声明的运行时版本。',
        prefer_china_mirrors: false,
        selected_dependencies: [],
      });
    });
    expect(await screen.findByRole('alert')).toHaveTextContent('已完成检查：缺少环境初始化模型。');
  });

  it('shows the Dockerfile and generates a cloud sandbox image on demand', async () => {
    const plannedResponse = {
      environment: {
        project_id: 'project-image',
        status: 'pending_image_build',
        sandbox_enabled: true,
        sandbox_provider: 'cloud_sandbox_manager',
        file_provider: 'harness',
        analysis_summary: '运行环境分析和 Dockerfile 生成完成。',
      },
      images: [
        {
          id: 'image-plan-1',
          environment_key: 'workspace',
          environment_type: 'workspace',
          display_name: 'Project Workspace',
          service_id: 'workspace',
          service_role: 'workspace',
          mcp_policy: {
            managed_by: 'system',
            attachment: 'workspace_gateway_target',
            filesystem: true,
            terminal: true,
          },
          image_provider: 'cloud_sandbox_manager',
          status: 'planned',
          dockerfile: 'FROM node:24\nRUN corepack enable',
          ports: [],
          env_vars: {},
        },
        {
          id: 'image-application-1',
          environment_key: 'application_runtime',
          environment_type: 'application',
          display_name: 'Application runtime',
          service_id: 'application_runtime',
          service_role: 'application',
          mcp_policy: {
            managed_by: 'system',
            attachment: 'none',
            filesystem: false,
            terminal: false,
          },
          image_provider: 'cloud_sandbox_manager',
          status: 'planned',
          ports: [],
          env_vars: {},
        },
      ],
    };
    const readyResponse = {
      ...plannedResponse,
      environment: { ...plannedResponse.environment, status: 'ready' },
      images: plannedResponse.images.map((image) => (
        image.service_role === 'workspace'
          ? {
            ...image,
            image_ref: 'chatos-sandbox-agent:node-24-project-image',
            status: 'ready',
          }
          : image
      )),
    };
    const submittedResponse = {
      ...plannedResponse,
      images: plannedResponse.images.map((image) => (
        image.service_role === 'workspace'
          ? { ...image, status: 'building' }
          : image
      )),
    };
    const generateProjectRuntimeEnvironmentImage = vi.fn(async () => submittedResponse);
    const client = {
      getProjectRuntimeEnvironment: vi.fn()
        .mockResolvedValueOnce(plannedResponse)
        .mockResolvedValue(readyResponse),
      getProjectRuntimeEnvironmentProgress: vi.fn(async () => ({ status: 'succeeded' })),
      analyzeProjectRuntimeEnvironment: vi.fn(async () => plannedResponse),
      generateProjectRuntimeEnvironmentImage,
    } as unknown as ApiClient;

    render(
      <ApiClientProvider client={client}>
        <I18nProvider>
          <CloudProjectRuntimeEnvironmentPanel
            projectId="project-image"
            projectName="Cloud image project"
            projectSourceType="cloud"
          />
        </I18nProvider>
      </ApiClientProvider>,
    );

    expect(await screen.findByText('cloud_sandbox_manager')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '查看 Dockerfile' }));
    expect(screen.getByText('FROM node:24', { exact: false })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '准备工作区镜像与依赖镜像' }));

    await waitFor(() => {
      expect(generateProjectRuntimeEnvironmentImage).toHaveBeenCalledWith(
        'project-image',
        'image-plan-1',
      );
    });
    const preparedButton = await screen.findByRole('button', { name: '工作区镜像与依赖镜像已准备' });
    expect(preparedButton).toBeDisabled();
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

  it('clears stale running progress once the environment is already ready', async () => {
    const analyzingResponse = {
      environment: {
        project_id: 'project-stale-progress',
        status: 'analyzing',
        sandbox_enabled: true,
        sandbox_provider: 'cloud_sandbox_manager',
        file_provider: 'harness',
        last_agent_run_id: 'agent-run-stale',
        updated_at: '2026-08-11T13:20:58Z',
      },
      images: [],
    };
    const readyResponse = {
      environment: {
        project_id: 'project-stale-progress',
        status: 'ready',
        sandbox_enabled: true,
        sandbox_provider: 'cloud_sandbox_manager',
        file_provider: 'harness',
        analysis_summary: '工作区镜像与依赖镜像已准备。',
        last_agent_run_id: 'agent-run-stale',
        updated_at: '2026-08-11T13:21:10Z',
      },
      images: [{
        id: 'dependency-postgres',
        environment_key: 'postgresql',
        environment_type: 'dependency',
        display_name: 'PostgreSQL 16',
        service_id: 'postgresql',
        service_role: 'dependency',
        mcp_policy: {
          managed_by: 'system',
          attachment: 'none',
          filesystem: false,
          terminal: false,
        },
        image_provider: 'cloud_sandbox_manager',
        image_ref: 'postgres:16-alpine',
        status: 'ready',
        ports: [],
        env_vars: {},
      }],
    };
    const getProjectRuntimeEnvironment = vi.fn()
      .mockResolvedValueOnce(analyzingResponse)
      .mockResolvedValue(readyResponse);
    const getProjectRuntimeEnvironmentProgress = vi.fn(async () => ({
      project_id: 'project-stale-progress',
      run_id: 'agent-run-stale',
      phase: 'running_agent_analysis',
      status: 'running',
      progress_percent: 40,
      provider: 'cloud_sandbox_manager',
      updated_at: '2026-08-11T13:21:00Z',
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
            projectId="project-stale-progress"
            projectName="Stale progress project"
            projectSourceType="cloud"
          />
        </I18nProvider>
      </ApiClientProvider>,
    );

    await waitFor(() => {
      expect(screen.getByRole('button', { name: '初始化/重新分析' })).toBeEnabled();
    });
    expect(screen.queryByText('镜像初始化进度')).not.toBeInTheDocument();
    expect(screen.getByText('工作区镜像与依赖镜像已准备。')).toBeInTheDocument();
  });
});
