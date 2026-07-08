// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import React from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import AgentManager from './AgentManager';
import AiModelManager from './AiModelManager';
import ApplicationsPanel from './ApplicationsPanel';
import McpManager from './McpManager';
import { I18nProvider } from '../i18n/I18nProvider';
import { ApiClientProvider } from '../lib/api/ApiClientContext';
import type ApiClient from '../lib/api/client';
import { useChatStoreResolved } from '../lib/store/ChatStoreContext';
import type { AgentConfig, AiModelConfig, Application, McpConfig } from '../types';
import { DialogProvider } from './ui/DialogProvider';

vi.mock('../lib/store/ChatStoreContext', async () => {
  const actual = await vi.importActual<typeof import('../lib/store/ChatStoreContext')>('../lib/store/ChatStoreContext');
  return {
    ...actual,
    useChatStoreResolved: vi.fn(),
  };
});

const mockedUseChatStoreResolved = vi.mocked(useChatStoreResolved);

const createApiClientStub = () => ({
  listSkillPlugins: vi.fn(async () => []),
  listSkills: vi.fn(async () => []),
  getMcpConfigResourceByCommand: vi.fn(async () => ({ success: true, config: null })),
} as unknown as ApiClient);

const renderWithProviders = (ui: React.ReactElement, client = createApiClientStub()) => render(
  <ApiClientProvider client={client}>
    <I18nProvider>
      <DialogProvider>
        {ui}
      </DialogProvider>
    </I18nProvider>
  </ApiClientProvider>,
);

const sampleAiModel: AiModelConfig = {
  id: 'model-1',
  name: 'Vision GPT',
  provider: 'gpt',
  base_url: 'https://api.example.com/v1',
  api_key: '',
  has_api_key: true,
  model_name: 'gpt-4.1',
  thinking_level: 'medium',
  enabled: true,
  supports_images: true,
  supports_reasoning: true,
  supports_responses: true,
  createdAt: new Date('2026-06-01T00:00:00Z'),
  updatedAt: new Date('2026-06-01T00:00:00Z'),
};

const sampleMcpConfig: McpConfig = {
  id: 'mcp-1',
  name: 'filesystem',
  command: 'npx @modelcontextprotocol/server-filesystem /tmp',
  type: 'stdio',
  args: [],
  cwd: '/tmp',
  enabled: true,
  createdAt: new Date('2026-06-01T00:00:00Z'),
  updatedAt: new Date('2026-06-01T00:00:00Z'),
};

const sampleAgent: AgentConfig = {
  id: 'agent-1',
  name: '工程助理',
  description: '负责日常协作',
  category: '研发',
  ai_model_config_id: '',
  enabled: true,
  role_definition: '帮助研发工作',
  plugin_sources: [],
  skill_ids: [],
  default_skill_ids: [],
  skills: [],
  mcp_policy: null,
  project_policy: null,
  app_ids: [],
  createdAt: new Date('2026-06-01T00:00:00Z'),
  updatedAt: new Date('2026-06-01T00:00:00Z'),
};

const sampleApplication: Application = {
  id: 'app-1',
  name: '运维看板',
  url: 'https://ops.example.com',
  iconUrl: '',
  createdAt: new Date('2026-06-01T00:00:00Z'),
  updatedAt: new Date('2026-06-01T00:00:00Z'),
};

describe('manager dialogs integration', () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    mockedUseChatStoreResolved.mockReset();
  });

  it('opens add and edit dialogs from AiModelManager', () => {
    const store = {
      aiModelConfigs: [sampleAiModel],
      loadAiModelConfigs: vi.fn(async () => undefined),
      updateAiModelConfig: vi.fn(async () => undefined),
      deleteAiModelConfig: vi.fn(async () => undefined),
    };

    renderWithProviders(<AiModelManager onClose={vi.fn()} store={() => store} />);

    fireEvent.click(screen.getByRole('button', { name: '添加 AI 模型' }));
    expect(screen.getByRole('dialog', { name: '添加 AI 模型' })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: 'Escape' });

    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
    expect(screen.getByRole('dialog', { name: '编辑 AI 模型' })).toBeInTheDocument();
  });

  it('opens add and edit dialogs from McpManager', () => {
    const store = {
      mcpConfigs: [sampleMcpConfig],
      updateMcpConfig: vi.fn(async () => sampleMcpConfig),
      deleteMcpConfig: vi.fn(async () => undefined),
      loadMcpConfigs: vi.fn(async () => undefined),
    };

    renderWithProviders(<McpManager onClose={vi.fn()} store={() => store} />);

    fireEvent.click(screen.getByRole('button', { name: /\+ 添加 MCP 服务器|添加 MCP 服务器/ }));
    expect(screen.getByRole('dialog', { name: '添加新服务器' })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: 'Escape' });

    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
    expect(screen.getByRole('dialog', { name: '编辑服务器' })).toBeInTheDocument();
  });

  it('opens add and edit dialogs from AgentManager', () => {
    const store = {
      agents: [sampleAgent],
      aiModelConfigs: [],
      loadAgents: vi.fn(async () => undefined),
      loadAiModelConfigs: vi.fn(async () => undefined),
      createAgent: vi.fn(async () => sampleAgent),
      updateAgent: vi.fn(async () => sampleAgent),
      deleteAgent: vi.fn(async () => undefined),
      aiCreateAgent: vi.fn(async () => sampleAgent),
    };

    renderWithProviders(<AgentManager onClose={vi.fn()} store={() => store} />);

    fireEvent.click(screen.getByRole('button', { name: '新建智能体' }));
    expect(screen.getByRole('dialog', { name: '新建智能体' })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: 'Escape' });

    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
    expect(screen.getByRole('dialog', { name: '编辑智能体' })).toBeInTheDocument();
  });

  it('opens add and edit dialogs from ApplicationsPanel manage view', () => {
    mockedUseChatStoreResolved.mockReturnValue({
      applications: [sampleApplication],
      loadApplications: vi.fn(async () => undefined),
      createApplication: vi.fn(async () => undefined),
      updateApplication: vi.fn(async () => undefined),
      deleteApplication: vi.fn(async () => undefined),
    } as never);

    renderWithProviders(
      <ApplicationsPanel
        isOpen
        manageOnly
        layout="embedded"
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新增应用' }));
    expect(screen.getByRole('dialog', { name: '新增应用' })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: 'Escape' });

    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
    expect(screen.getByRole('dialog', { name: '编辑应用' })).toBeInTheDocument();
  });
});
