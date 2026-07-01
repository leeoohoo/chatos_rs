// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Alert,
  Button,
  Descriptions,
  Form,
  Input,
  InputNumber,
  Segmented,
  Space,
  Statistic,
  Switch,
  Tag,
  Typography,
} from 'antd';
import type { FormInstance } from 'antd';

import { McpPromptPreviewCard } from '../../components/McpPromptPreviewCard';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  McpPromptPreviewResponse,
  McpServerInfo,
  SystemConfigResponse,
  TaskRunnerInternalPromptPreviewResponse,
  TaskRunnerSkillResponse,
} from '../../types';
import {
  errorMessage,
  formatSecondsFromMs,
  type RuntimeSettingsFormValues,
  type SettingsPromptLocale,
} from './settingsPageUtils';

type SettingsOverviewTabProps = {
  t: TranslateFn;
  config?: SystemConfigResponse;
  mcpServer?: McpServerInfo;
  implementedBuiltinCount: number;
  runtimeDefaultCount: number;
  form: FormInstance<RuntimeSettingsFormValues>;
  saveLoading: boolean;
  onOpenMcpCatalog: () => void;
  onSubmit: (values: RuntimeSettingsFormValues) => void;
};

export function SettingsOverviewTab({
  t,
  config,
  mcpServer,
  implementedBuiltinCount,
  runtimeDefaultCount,
  form,
  saveLoading,
  onOpenMcpCatalog,
  onSubmit,
}: SettingsOverviewTabProps) {
  const storeModeColor =
    config?.store_mode === 'mongo'
      ? 'green'
      : config?.store_mode === 'sqlite'
        ? 'blue'
        : 'gold';

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space size="large" wrap>
        <Statistic title="Builtin MCP" value={implementedBuiltinCount} />
        <Statistic title="Runtime Default" value={runtimeDefaultCount} />
        <Statistic title={t('settings.externalTools')} value={mcpServer?.tool_names.length || 0} />
      </Space>

      {config ? (
        <Descriptions bordered column={1} size="small">
          <Descriptions.Item label={t('settings.httpListen')}>
            {config.host}:{config.port}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.storeMode')}>
            <Tag color={storeModeColor}>{config.store_mode}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.database')}>{config.database_url}</Descriptions.Item>
          <Descriptions.Item label="Memory Engine">
            <Tag color={config.memory_engine_configured ? 'success' : 'default'}>
              {config.memory_engine_configured ? t('common.configured') : t('common.notConfigured')}
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
          <Descriptions.Item label="Memory Timeout">{config.memory_timeout_ms} ms</Descriptions.Item>
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
          <Descriptions.Item label={t('settings.executionEnvironmentMode')}>
            <Tag color={config.execution_environment_mode === 'cloud' ? 'purple' : 'blue'}>
              {config.execution_environment_mode === 'cloud'
                ? t('settings.executionMode.cloud')
                : t('settings.executionMode.local')}
            </Tag>
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.sandboxEnabled')}>
            <Tag color={config.sandbox_enabled ? 'green' : 'default'}>
              {config.sandbox_enabled
                ? t('settings.sandboxSwitchOn')
                : t('settings.sandboxSwitchOff')}
            </Tag>
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.sandboxManagerBaseUrl')}>
            {config.sandbox_manager_base_url || '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.sandboxLeaseTtl')}>
            {config.sandbox_lease_ttl_seconds} s
          </Descriptions.Item>
        </Descriptions>
      ) : null}

      {config ? (
        <RuntimeSettingsForm
          t={t}
          form={form}
          saveLoading={saveLoading}
          onSubmit={onSubmit}
        />
      ) : null}

      {mcpServer ? (
        <Descriptions
          title={t('settings.mcpService')}
          bordered
          column={1}
          size="small"
          extra={
            <Button size="small" onClick={onOpenMcpCatalog}>
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
  );
}

type RuntimeSettingsFormProps = {
  t: TranslateFn;
  form: FormInstance<RuntimeSettingsFormValues>;
  saveLoading: boolean;
  onSubmit: (values: RuntimeSettingsFormValues) => void;
};

function RuntimeSettingsForm({ t, form, saveLoading, onSubmit }: RuntimeSettingsFormProps) {
  return (
    <Space direction="vertical" size="middle" style={{ width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={5} style={{ margin: 0 }}>
          {t('settings.runtimeSection')}
        </Typography.Title>
        <Typography.Text type="secondary">{t('settings.roundLimitHelp')}</Typography.Text>
        <Typography.Text type="secondary">{t('settings.executionTimeoutHelp')}</Typography.Text>
        <Typography.Text type="secondary">{t('settings.toolResultBudgetHelp')}</Typography.Text>
      </Space>
      <Form<RuntimeSettingsFormValues> layout="vertical" form={form} onFinish={onSubmit}>
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
          <Form.Item
            name="execution_environment_mode"
            label={t('settings.executionEnvironmentMode')}
            rules={[
              {
                required: true,
                message: t('settings.executionEnvironmentModeRequired'),
              },
            ]}
          >
            <Segmented
              options={[
                {
                  label: t('settings.executionMode.local'),
                  value: 'local',
                },
                {
                  label: t('settings.executionMode.cloud'),
                  value: 'cloud',
                },
              ]}
            />
          </Form.Item>
          <Form.Item
            name="sandbox_enabled"
            label={t('settings.sandboxEnabled')}
            valuePropName="checked"
            help={t('settings.sandboxEnabledHelp')}
          >
            <Switch
              checkedChildren={t('settings.sandboxSwitchOn')}
              unCheckedChildren={t('settings.sandboxSwitchOff')}
            />
          </Form.Item>
          <Form.Item shouldUpdate noStyle>
            {({ getFieldValue }) =>
              getFieldValue('sandbox_enabled') ? (
                <>
                  <Form.Item
                    name="sandbox_manager_base_url"
                    label={t('settings.sandboxManagerBaseUrl')}
                    rules={[
                      {
                        required: true,
                        message: t('settings.sandboxManagerBaseUrlRequired'),
                      },
                    ]}
                  >
                    <Input style={{ width: 280 }} placeholder="http://127.0.0.1:8095" />
                  </Form.Item>
                  <Form.Item
                    name="sandbox_lease_ttl_seconds"
                    label={t('settings.sandboxLeaseTtl')}
                    rules={[
                      {
                        required: true,
                        message: t('settings.sandboxLeaseTtlRequired'),
                      },
                    ]}
                  >
                    <InputNumber min={60} precision={0} style={{ width: 220 }} />
                  </Form.Item>
                </>
              ) : null
            }
          </Form.Item>
          <Button type="primary" onClick={() => form.submit()} loading={saveLoading}>
            {t('common.save')}
          </Button>
        </Space>
      </Form>
    </Space>
  );
}

type SettingsSkillTabProps = {
  t: TranslateFn;
  locale: string;
  endpoint: string;
  title: string;
  subtitle: string;
  contentTitle: string;
  data?: TaskRunnerSkillResponse;
  error: unknown;
  refreshLoading: boolean;
  onLocaleChange: (value: SettingsPromptLocale) => void;
  onRefresh: () => void;
};

export function SettingsSkillTab({
  t,
  locale,
  endpoint,
  title,
  subtitle,
  contentTitle,
  data,
  error,
  refreshLoading,
  onLocaleChange,
  onRefresh,
}: SettingsSkillTabProps) {
  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <SettingsPromptToolbar
        t={t}
        locale={locale}
        title={title}
        subtitle={subtitle}
        refreshLoading={refreshLoading}
        onLocaleChange={onLocaleChange}
        onRefresh={onRefresh}
      />

      <Alert message={t('settings.externalSkillLocaleNote')} type="info" showIcon />

      <Descriptions bordered column={1} size="small">
        <Descriptions.Item label={t('settings.externalSkillEndpoint')}>
          <Typography.Text code copyable>
            {endpoint}
          </Typography.Text>
        </Descriptions.Item>
        <Descriptions.Item label={t('settings.externalSkillName')}>
          {data?.name || '-'}
        </Descriptions.Item>
        <Descriptions.Item label={t('settings.externalSkillLocale')}>
          {data?.locale || locale}
        </Descriptions.Item>
      </Descriptions>

      {error ? <Alert type="error" showIcon message={errorMessage(error)} /> : null}

      <PromptContentCard
        title={contentTitle}
        content={data?.content}
        emptyText={t('settings.noPreview')}
      />
    </Space>
  );
}

type SettingsInternalPromptsTabProps = {
  t: TranslateFn;
  locale: string;
  prompts?: TaskRunnerInternalPromptPreviewResponse;
  promptsError: unknown;
  builtinPreview?: McpPromptPreviewResponse;
  builtinPreviewError: unknown;
  refreshLoading: boolean;
  onLocaleChange: (value: SettingsPromptLocale) => void;
  onRefresh: () => void;
};

export function SettingsInternalPromptsTab({
  t,
  locale,
  prompts,
  promptsError,
  builtinPreview,
  builtinPreviewError,
  refreshLoading,
  onLocaleChange,
  onRefresh,
}: SettingsInternalPromptsTabProps) {
  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <SettingsPromptToolbar
        t={t}
        locale={locale}
        title={t('settings.internalPromptsTitle')}
        subtitle={t('settings.internalPromptsSubtitle')}
        refreshLoading={refreshLoading}
        onLocaleChange={onLocaleChange}
        onRefresh={onRefresh}
      />

      <Alert message={t('settings.promptLanguageScope')} type="info" showIcon />

      <Space wrap>
        <Tag color="success">{t('common.enabled')}</Tag>
        <Tag color="blue">effective</Tag>
        <Tag>{locale}</Tag>
        <Tag color="processing">{t('settings.runtimeDefaultPreset')}</Tag>
      </Space>

      {promptsError ? <Alert type="error" showIcon message={errorMessage(promptsError)} /> : null}

      <PromptContentCard
        title={t('settings.internalTaskPrompt')}
        description={t('settings.internalTaskPromptHelp')}
        content={prompts?.task_prompt_template}
        emptyText={t('settings.noPreview')}
      />

      <PromptContentCard
        title={t('settings.internalGlobalPrompt')}
        description={t('settings.internalGlobalPromptHelp')}
        content={prompts?.global_execution_prompt}
        emptyText={t('settings.noPreview')}
      />

      <Space direction="vertical" size="small" style={{ width: '100%' }}>
        <Typography.Title level={5} style={{ margin: 0 }}>
          {t('settings.internalBuiltinPrompt')}
        </Typography.Title>
        <Typography.Text type="secondary">{t('settings.internalBuiltinPromptHelp')}</Typography.Text>
        {builtinPreviewError ? (
          <Alert type="error" showIcon message={errorMessage(builtinPreviewError)} />
        ) : null}
        {builtinPreview ? <McpPromptPreviewCard preview={builtinPreview} /> : null}
      </Space>

      <PromptContentCard
        title={t('settings.internalProcessPrompt')}
        description={t('settings.internalProcessPromptHelp')}
        content={prompts?.process_log_system_prompt}
        emptyText={t('settings.noPreview')}
      />

      <PromptNotes title={t('settings.promptNotes')} notes={prompts?.notes || []} />
    </Space>
  );
}

type SettingsPromptToolbarProps = {
  t: TranslateFn;
  locale: string;
  title: string;
  subtitle: string;
  refreshLoading: boolean;
  onLocaleChange: (value: SettingsPromptLocale) => void;
  onRefresh: () => void;
};

function SettingsPromptToolbar({
  t,
  locale,
  title,
  subtitle,
  refreshLoading,
  onLocaleChange,
  onRefresh,
}: SettingsPromptToolbarProps) {
  return (
    <Space style={{ justifyContent: 'space-between', width: '100%' }} wrap>
      <Space direction="vertical" size={0}>
        <Typography.Title level={5} style={{ margin: 0 }}>
          {title}
        </Typography.Title>
        <Typography.Text type="secondary">{subtitle}</Typography.Text>
      </Space>
      <Space wrap>
        <Segmented
          value={locale}
          onChange={(value) => onLocaleChange(value as SettingsPromptLocale)}
          options={[
            { label: t('mcp.promptLanguage.zhCN'), value: 'zh-CN' },
            { label: t('mcp.promptLanguage.enUS'), value: 'en-US' },
          ]}
        />
        <Button onClick={onRefresh} loading={refreshLoading}>
          {t('common.refresh')}
        </Button>
      </Space>
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

function PromptNotes({ title, notes }: { title: string; notes: string[] }) {
  return (
    <Space direction="vertical" size="small" style={{ width: '100%' }}>
      <Typography.Title level={5} style={{ margin: 0 }}>
        {title}
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
          {notes.map((note) => (
            <li key={note} style={{ marginBottom: 8 }}>
              <Typography.Text>{note}</Typography.Text>
            </li>
          ))}
        </ul>
      </div>
    </Space>
  );
}
