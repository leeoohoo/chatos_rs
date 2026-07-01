// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { Alert, Form, Space, Tabs, Typography, message } from 'antd';
import dayjs from 'dayjs';

import { api, buildApiUrl } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import {
  SettingsInternalPromptsTab,
  SettingsOverviewTab,
  SettingsSkillTab,
} from './settings/SettingsSections';
import {
  millisecondsToWholeSeconds,
  type RuntimeSettingsFormValues,
  type SettingsPromptLocale,
  type SettingsTabKey,
} from './settings/settingsPageUtils';

export function SettingsPage() {
  const { locale, t } = useI18n();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [messageApi, contextHolder] = message.useMessage();
  const [form] = Form.useForm<RuntimeSettingsFormValues>();
  const [activeTab, setActiveTab] = useState<SettingsTabKey>('overview');
  const [skillLocale, setSkillLocale] = useState(locale);
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
  const skillQuery = useQuery({
    queryKey: ['task-runner-skill', skillLocale],
    queryFn: () => api.getTaskRunnerSkill(skillLocale),
    enabled: activeTab === 'external-skill',
  });
  const planSkillQuery = useQuery({
    queryKey: ['task-runner-plan-skill', skillLocale],
    queryFn: () => api.getTaskRunnerSkill(skillLocale, 'chatos_plan'),
    enabled: activeTab === 'plan-skill',
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
  const updateSystemConfigMutation = useMutation({
    mutationFn: api.updateSystemConfig,
    onSuccess: async (nextConfig) => {
      queryClient.setQueryData(['system-config'], nextConfig);
      await queryClient.invalidateQueries({ queryKey: ['system-config'] });
      messageApi.success(t('settings.saved'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const health = healthQuery.data;
  const config = configQuery.data;
  const mcpServer = mcpServerQuery.data;
  const mcpCatalog = mcpCatalogQuery.data || [];
  const implementedBuiltinCount = mcpCatalog.filter((entry) => entry.implemented).length;
  const runtimeDefaultCount = mcpCatalog.filter((entry) => entry.runtime_default).length;
  const skillEndpoint = buildApiUrl(`/api/skills/task-runner?lang=${encodeURIComponent(skillLocale)}`);
  const planSkillEndpoint = buildApiUrl(
    `/api/skills/task-runner?lang=${encodeURIComponent(skillLocale)}&profile=chatos_plan`,
  );

  useEffect(() => {
    if (!config) {
      return;
    }
    form.setFieldsValue({
      task_execution_max_iterations: config.task_execution_max_iterations,
      execution_timeout_seconds: millisecondsToWholeSeconds(config.execution_timeout_ms),
      tool_result_model_max_chars: config.tool_result_model_max_chars,
      tool_results_model_total_max_chars: config.tool_results_model_total_max_chars,
      execution_environment_mode:
        config.execution_environment_mode === 'cloud' ? 'cloud' : 'local',
      sandbox_enabled: config.sandbox_enabled,
      sandbox_manager_base_url: config.sandbox_manager_base_url,
      sandbox_lease_ttl_seconds: config.sandbox_lease_ttl_seconds,
    });
  }, [config, form]);

  function handleRuntimeSettingsSubmit(values: RuntimeSettingsFormValues) {
    updateSystemConfigMutation.mutate({
      task_execution_max_iterations: values.task_execution_max_iterations,
      execution_timeout_ms:
        values.execution_timeout_seconds === undefined
          ? undefined
          : Math.max(1, Math.round(values.execution_timeout_seconds * 1000)),
      tool_result_model_max_chars: values.tool_result_model_max_chars,
      tool_results_model_total_max_chars: values.tool_results_model_total_max_chars,
      execution_environment_mode: values.execution_environment_mode,
      sandbox_enabled: values.sandbox_enabled,
      sandbox_manager_base_url: values.sandbox_manager_base_url,
      sandbox_lease_ttl_seconds: values.sandbox_lease_ttl_seconds,
    });
  }

  function handleSkillLocaleChange(value: SettingsPromptLocale) {
    setSkillLocale(value);
  }

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
          form={form}
          saveLoading={updateSystemConfigMutation.isPending}
          onOpenMcpCatalog={() => navigate('/mcp')}
          onSubmit={handleRuntimeSettingsSubmit}
        />
      ),
    },
    {
      key: 'external-skill',
      label: t('settings.tabs.externalSkill'),
      children: (
        <SettingsSkillTab
          t={t}
          locale={skillLocale}
          endpoint={skillEndpoint}
          title={t('settings.externalSkillTitle')}
          subtitle={t('settings.externalSkillSubtitle')}
          contentTitle={t('settings.externalSkillContent')}
          data={skillQuery.data}
          error={skillQuery.error}
          refreshLoading={skillQuery.isFetching}
          onLocaleChange={handleSkillLocaleChange}
          onRefresh={() => {
            skillQuery.refetch();
          }}
        />
      ),
    },
    {
      key: 'plan-skill',
      label: t('settings.tabs.planSkill'),
      children: (
        <SettingsSkillTab
          t={t}
          locale={skillLocale}
          endpoint={planSkillEndpoint}
          title={t('settings.planSkillTitle')}
          subtitle={t('settings.planSkillSubtitle')}
          contentTitle={t('settings.planSkillContent')}
          data={planSkillQuery.data}
          error={planSkillQuery.error}
          refreshLoading={planSkillQuery.isFetching}
          onLocaleChange={handleSkillLocaleChange}
          onRefresh={() => {
            planSkillQuery.refetch();
          }}
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
      {contextHolder}
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
