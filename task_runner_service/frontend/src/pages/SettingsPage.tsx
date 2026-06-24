import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import {
  Alert,
  Button,
  Descriptions,
  Form,
  InputNumber,
  Segmented,
  Space,
  Statistic,
  Tabs,
  Tag,
  Typography,
  message,
} from 'antd';
import dayjs from 'dayjs';

import { api, buildApiUrl } from '../api/client';
import { McpPromptPreviewCard } from '../components/McpPromptPreviewCard';
import { useI18n } from '../i18n/I18nProvider';

type RuntimeSettingsFormValues = {
  task_execution_max_iterations?: number;
  execution_timeout_seconds?: number;
  tool_result_model_max_chars?: number;
  tool_results_model_total_max_chars?: number;
};

type SettingsTabKey = 'overview' | 'external-skill' | 'internal-prompts';

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
        init_mode: 'builtin_only',
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
  const storeModeColor =
    config?.store_mode === 'mongo'
      ? 'green'
      : config?.store_mode === 'sqlite'
        ? 'blue'
        : 'gold';
  const skillEndpoint = buildApiUrl(`/api/skills/task-runner?lang=${encodeURIComponent(skillLocale)}`);

  useEffect(() => {
    if (!config) {
      return;
    }
    form.setFieldsValue({
      task_execution_max_iterations: config.task_execution_max_iterations,
      execution_timeout_seconds: millisecondsToWholeSeconds(config.execution_timeout_ms),
      tool_result_model_max_chars: config.tool_result_model_max_chars,
      tool_results_model_total_max_chars: config.tool_results_model_total_max_chars,
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
    });
  }

  const tabItems = [
    {
      key: 'overview',
      label: t('settings.tabs.overview'),
      children: (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Space size="large" wrap>
            <Statistic title="Builtin MCP" value={implementedBuiltinCount} />
            <Statistic title="Runtime Default" value={runtimeDefaultCount} />
            <Statistic
              title={t('settings.externalTools')}
              value={mcpServer?.tool_names.length || 0}
            />
          </Space>

          {config ? (
            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label={t('settings.httpListen')}>
                {config.host}:{config.port}
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.storeMode')}>
                <Tag color={storeModeColor}>{config.store_mode}</Tag>
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.database')}>
                {config.database_url}
              </Descriptions.Item>
              <Descriptions.Item label="Memory Engine">
                <Tag color={config.memory_engine_configured ? 'success' : 'default'}>
                  {config.memory_engine_configured
                    ? t('common.configured')
                    : t('common.notConfigured')}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Memory Base URL">
                {config.memory_engine_base_url || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Memory Source ID">
                {config.memory_engine_source_id}
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.defaultTenant')}>
                {config.default_tenant_id}
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.defaultSubject')}>
                {config.default_subject_id}
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.defaultWorkspace')}>
                {config.default_workspace_dir}
              </Descriptions.Item>
              <Descriptions.Item label="Memory Timeout">
                {config.memory_timeout_ms} ms
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.defaultExecutionTimeout')}>
                {formatSecondsFromMs(config.default_execution_timeout_ms)}
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.currentExecutionTimeout')}>
                {formatSecondsFromMs(config.execution_timeout_ms)}
              </Descriptions.Item>
              <Descriptions.Item label="Scheduler Poll Interval">
                {config.scheduler_poll_interval_ms} ms
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.defaultRoundLimit')}>
                {config.default_task_execution_max_iterations}
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.currentRoundLimit')}>
                {config.task_execution_max_iterations}
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.defaultToolResultLimit')}>
                {config.default_tool_result_model_max_chars}
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.currentToolResultLimit')}>
                {config.tool_result_model_max_chars}
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.defaultToolResultsBudget')}>
                {config.default_tool_results_model_total_max_chars}
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.currentToolResultsBudget')}>
                {config.tool_results_model_total_max_chars}
              </Descriptions.Item>
            </Descriptions>
          ) : null}

          {config ? (
            <Space direction="vertical" size="middle" style={{ width: '100%' }}>
              <Space direction="vertical" size={0}>
                <Typography.Title level={5} style={{ margin: 0 }}>
                  {t('settings.runtimeSection')}
                </Typography.Title>
                <Typography.Text type="secondary">
                  {t('settings.roundLimitHelp')}
                </Typography.Text>
                <Typography.Text type="secondary">
                  {t('settings.executionTimeoutHelp')}
                </Typography.Text>
                <Typography.Text type="secondary">
                  {t('settings.toolResultBudgetHelp')}
                </Typography.Text>
              </Space>
              <Form<RuntimeSettingsFormValues>
                layout="vertical"
                form={form}
                onFinish={handleRuntimeSettingsSubmit}
              >
                <Space align="end" wrap>
                  <Form.Item
                    name="task_execution_max_iterations"
                    label={t('settings.currentRoundLimit')}
                    rules={[
                      {
                        required: true,
                        message: t('settings.roundLimitRequired'),
                      },
                    ]}
                  >
                    <InputNumber min={1} style={{ width: 220 }} />
                  </Form.Item>
                  <Form.Item
                    name="execution_timeout_seconds"
                    label={t('settings.currentExecutionTimeout')}
                    rules={[
                      {
                        required: true,
                        message: t('settings.executionTimeoutRequired'),
                      },
                    ]}
                  >
                    <InputNumber min={1} precision={0} style={{ width: 220 }} />
                  </Form.Item>
                  <Form.Item
                    name="tool_result_model_max_chars"
                    label={t('settings.currentToolResultLimit')}
                    rules={[
                      {
                        required: true,
                        message: t('settings.toolResultLimitRequired'),
                      },
                    ]}
                  >
                    <InputNumber min={1} style={{ width: 220 }} />
                  </Form.Item>
                  <Form.Item
                    name="tool_results_model_total_max_chars"
                    label={t('settings.currentToolResultsBudget')}
                    rules={[
                      {
                        required: true,
                        message: t('settings.toolResultsBudgetRequired'),
                      },
                    ]}
                  >
                    <InputNumber min={1} style={{ width: 220 }} />
                  </Form.Item>
                  <Button
                    type="primary"
                    onClick={() => form.submit()}
                    loading={updateSystemConfigMutation.isPending}
                  >
                    {t('common.save')}
                  </Button>
                </Space>
              </Form>
            </Space>
          ) : null}

          {mcpServer ? (
            <Descriptions
              title={t('settings.mcpService')}
              bordered
              column={1}
              size="small"
              extra={
                <Button size="small" onClick={() => navigate('/mcp')}>
                  {t('settings.openMcpCatalog')}
                </Button>
              }
            >
              <Descriptions.Item label="Server Name">{mcpServer.server_name}</Descriptions.Item>
              <Descriptions.Item label="Transports">
                <Space wrap>
                  {mcpServer.transports.map((transport) => (
                    <Tag key={transport} color="blue">
                      {transport}
                    </Tag>
                  ))}
                </Space>
              </Descriptions.Item>
              <Descriptions.Item label="HTTP Endpoint">
                {mcpServer.http_endpoint_path || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="STDIO Command">
                {mcpServer.stdio_command || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="STDIO Args">
                {mcpServer.stdio_args.length ? mcpServer.stdio_args.join(' ') : '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('settings.exposedToolCount')}>
                {mcpServer.tool_names.length}
              </Descriptions.Item>
            </Descriptions>
          ) : null}
        </Space>
      ),
    },
    {
      key: 'external-skill',
      label: t('settings.tabs.externalSkill'),
      children: (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Space style={{ justifyContent: 'space-between', width: '100%' }} wrap>
            <Space direction="vertical" size={0}>
              <Typography.Title level={5} style={{ margin: 0 }}>
                {t('settings.externalSkillTitle')}
              </Typography.Title>
              <Typography.Text type="secondary">
                {t('settings.externalSkillSubtitle')}
              </Typography.Text>
            </Space>
            <Space wrap>
              <Segmented
                value={skillLocale}
                onChange={(value) => setSkillLocale(value as 'zh-CN' | 'en-US')}
                options={[
                  { label: t('mcp.promptLanguage.zhCN'), value: 'zh-CN' },
                  { label: t('mcp.promptLanguage.enUS'), value: 'en-US' },
                ]}
              />
              <Button onClick={() => skillQuery.refetch()} loading={skillQuery.isFetching}>
                {t('common.refresh')}
              </Button>
            </Space>
          </Space>

          <Alert message={t('settings.externalSkillLocaleNote')} type="info" showIcon />

          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label={t('settings.externalSkillEndpoint')}>
              <Typography.Text code copyable>
                {skillEndpoint}
              </Typography.Text>
            </Descriptions.Item>
            <Descriptions.Item label={t('settings.externalSkillName')}>
              {skillQuery.data?.name || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('settings.externalSkillLocale')}>
              {skillQuery.data?.locale || skillLocale}
            </Descriptions.Item>
          </Descriptions>

          {skillQuery.error ? (
            <Alert type="error" showIcon message={errorMessage(skillQuery.error)} />
          ) : null}

          <PromptContentCard
            title={t('settings.externalSkillContent')}
            content={skillQuery.data?.content}
            emptyText={t('settings.noPreview')}
          />
        </Space>
      ),
    },
    {
      key: 'internal-prompts',
      label: t('settings.tabs.internalPrompts'),
      children: (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Space style={{ justifyContent: 'space-between', width: '100%' }} wrap>
            <Space direction="vertical" size={0}>
              <Typography.Title level={5} style={{ margin: 0 }}>
                {t('settings.internalPromptsTitle')}
              </Typography.Title>
              <Typography.Text type="secondary">
                {t('settings.internalPromptsSubtitle')}
              </Typography.Text>
            </Space>
            <Space wrap>
              <Segmented
                value={internalPromptLocale}
                onChange={(value) => setInternalPromptLocale(value as 'zh-CN' | 'en-US')}
                options={[
                  { label: t('mcp.promptLanguage.zhCN'), value: 'zh-CN' },
                  { label: t('mcp.promptLanguage.enUS'), value: 'en-US' },
                ]}
              />
              <Button
                onClick={() => {
                  internalPromptsQuery.refetch();
                  builtinPromptPreviewQuery.refetch();
                }}
                loading={internalPromptsQuery.isFetching || builtinPromptPreviewQuery.isFetching}
              >
                {t('common.refresh')}
              </Button>
            </Space>
          </Space>

          <Alert message={t('settings.promptLanguageScope')} type="info" showIcon />

          <Space wrap>
            <Tag color="success">{t('common.enabled')}</Tag>
            <Tag>builtin_only</Tag>
            <Tag color="blue">effective</Tag>
            <Tag>{internalPromptLocale}</Tag>
            <Tag color="processing">{t('settings.runtimeDefaultPreset')}</Tag>
          </Space>

          {internalPromptsQuery.error ? (
            <Alert type="error" showIcon message={errorMessage(internalPromptsQuery.error)} />
          ) : null}

          <PromptContentCard
            title={t('settings.internalTaskPrompt')}
            description={t('settings.internalTaskPromptHelp')}
            content={internalPromptsQuery.data?.task_prompt_template}
            emptyText={t('settings.noPreview')}
          />

          <PromptContentCard
            title={t('settings.internalGlobalPrompt')}
            description={t('settings.internalGlobalPromptHelp')}
            content={internalPromptsQuery.data?.global_execution_prompt}
            emptyText={t('settings.noPreview')}
          />

          <Space direction="vertical" size="small" style={{ width: '100%' }}>
            <Typography.Title level={5} style={{ margin: 0 }}>
              {t('settings.internalBuiltinPrompt')}
            </Typography.Title>
            <Typography.Text type="secondary">
              {t('settings.internalBuiltinPromptHelp')}
            </Typography.Text>
            {builtinPromptPreviewQuery.error ? (
              <Alert
                type="error"
                showIcon
                message={errorMessage(builtinPromptPreviewQuery.error)}
              />
            ) : null}
            {builtinPromptPreviewQuery.data ? (
              <McpPromptPreviewCard preview={builtinPromptPreviewQuery.data} />
            ) : null}
          </Space>

          <PromptContentCard
            title={t('settings.internalProcessPrompt')}
            description={t('settings.internalProcessPromptHelp')}
            content={internalPromptsQuery.data?.process_log_system_prompt}
            emptyText={t('settings.noPreview')}
          />

          <Space direction="vertical" size="small" style={{ width: '100%' }}>
            <Typography.Title level={5} style={{ margin: 0 }}>
              {t('settings.promptNotes')}
            </Typography.Title>
            <div
              style={{
                background: '#fafafa',
                border: '1px solid #f0f0f0',
                borderRadius: 6,
                padding: 12,
              }}
            >
              <ul style={{ margin: 0, paddingInlineStart: 20 }}>
                {(internalPromptsQuery.data?.notes || []).map((note) => (
                  <li key={note} style={{ marginBottom: 8 }}>
                    <Typography.Text>{note}</Typography.Text>
                  </li>
                ))}
              </ul>
            </div>
          </Space>
        </Space>
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

function PromptContentCard({
  title,
  description,
  content,
  emptyText,
}: {
  title: string;
  description?: string;
  content?: string | null;
  emptyText: string;
}) {
  return (
    <Space direction="vertical" size="small" style={{ width: '100%' }}>
      <Typography.Title level={5} style={{ margin: 0 }}>
        {title}
      </Typography.Title>
      {description ? <Typography.Text type="secondary">{description}</Typography.Text> : null}
      <Typography.Paragraph
        style={{
          background: '#fafafa',
          padding: 12,
          borderRadius: 6,
          marginBottom: 0,
          whiteSpace: 'pre-wrap',
          fontFamily: 'monospace',
          border: '1px solid #f0f0f0',
        }}
      >
        {content || emptyText}
      </Typography.Paragraph>
    </Space>
  );
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function millisecondsToWholeSeconds(value: number): number {
  return Math.max(1, Math.ceil(value / 1000));
}

function formatSecondsFromMs(value: number): string {
  const seconds = value / 1000;
  return `${Number.isInteger(seconds) ? seconds : seconds.toFixed(1)} s`;
}
