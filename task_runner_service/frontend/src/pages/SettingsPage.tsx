// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { Alert, Space, Tabs, Typography } from 'antd';
import dayjs from 'dayjs';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import { SettingsInternalPromptsTab, SettingsOverviewTab } from './settings/SettingsSections';
import { type SettingsPromptLocale, type SettingsTabKey } from './settings/settingsPageUtils';

export function SettingsPage() {
  const { locale, t } = useI18n();
  const navigate = useNavigate();
  const [activeTab, setActiveTab] = useState<SettingsTabKey>('overview');
  const [internalPromptLocale, setInternalPromptLocale] = useState(locale);

  const healthQuery = useQuery({
    queryKey: ['health'],
    queryFn: api.health,
  });
  const configQuery = useQuery({
    queryKey: ['system-config'],
    queryFn: api.getSystemConfig,
  });
  const mcpServerQuery = useQuery({
    queryKey: ['mcp-server-info'],
    queryFn: api.getMcpServerInfo,
  });
  const mcpCatalogQuery = useQuery({
    queryKey: ['mcp-catalog'],
    queryFn: api.listMcpCatalog,
  });
  const internalPromptsQuery = useQuery({
    queryKey: ['task-runner-internal-prompts', internalPromptLocale],
    queryFn: () => api.getTaskRunnerInternalPrompts(internalPromptLocale),
    enabled: activeTab === 'internal-prompts',
  });
  const builtinPromptPreviewQuery = useQuery({
    queryKey: ['settings-mcp-prompt-preview', internalPromptLocale],
    queryFn: () =>
      api.previewMcpPrompt({
        enabled: true,
        init_mode: 'full',
        builtin_prompt_mode: 'effective',
        builtin_prompt_locale: internalPromptLocale,
      }),
    enabled: activeTab === 'internal-prompts',
  });

  const health = healthQuery.data;
  const config = configQuery.data;
  const mcpServer = mcpServerQuery.data;
  const mcpCatalog = mcpCatalogQuery.data || [];
  const implementedBuiltinCount = mcpCatalog.filter((entry) => entry.implemented).length;
  const runtimeDefaultCount = mcpCatalog.filter((entry) => entry.runtime_default).length;

  function handleInternalPromptLocaleChange(value: SettingsPromptLocale) {
    setInternalPromptLocale(value);
  }

  const tabItems = [
    {
      key: 'overview',
      label: t('settings.tabs.overview'),
      children: (
        <SettingsOverviewTab
          t={t}
          config={config}
          mcpServer={mcpServer}
          implementedBuiltinCount={implementedBuiltinCount}
          runtimeDefaultCount={runtimeDefaultCount}
          onOpenMcpCatalog={() => navigate('/mcp')}
        />
      ),
    },
    {
      key: 'internal-prompts',
      label: t('settings.tabs.internalPrompts'),
      children: (
        <SettingsInternalPromptsTab
          t={t}
          locale={internalPromptLocale}
          prompts={internalPromptsQuery.data}
          promptsError={internalPromptsQuery.error}
          builtinPreview={builtinPromptPreviewQuery.data}
          builtinPreviewError={builtinPromptPreviewQuery.error}
          refreshLoading={internalPromptsQuery.isFetching || builtinPromptPreviewQuery.isFetching}
          onLocaleChange={handleInternalPromptLocaleChange}
          onRefresh={() => {
            internalPromptsQuery.refetch();
            builtinPromptPreviewQuery.refetch();
          }}
        />
      ),
    },
  ];

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          {t('settings.title')}
        </Typography.Title>
        <Typography.Text type="secondary">{t('settings.subtitle')}</Typography.Text>
      </Space>

      {health ? (
        <Alert
          type={health.status === 'ok' ? 'success' : 'warning'}
          message={`${health.service} / ${health.status}`}
          description={t('settings.lastProbe', {
            time: dayjs(health.now).format('YYYY-MM-DD HH:mm:ss'),
          })}
          showIcon
        />
      ) : null}

      <Tabs
        activeKey={activeTab}
        onChange={(value) => setActiveTab(value as SettingsTabKey)}
        items={tabItems}
      />
    </Space>
  );
}
