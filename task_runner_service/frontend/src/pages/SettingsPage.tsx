import { useEffect } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import {
  Alert,
  Button,
  Descriptions,
  Form,
  InputNumber,
  Space,
  Statistic,
  Tag,
  Typography,
  message,
} from 'antd';
import dayjs from 'dayjs';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';

type RuntimeSettingsFormValues = {
  task_execution_max_iterations?: number;
  tool_result_model_max_chars?: number;
  tool_results_model_total_max_chars?: number;
};

export function SettingsPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [messageApi, contextHolder] = message.useMessage();
  const [form] = Form.useForm<RuntimeSettingsFormValues>();
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

  useEffect(() => {
    if (!config) {
      return;
    }
    form.setFieldsValue({
      task_execution_max_iterations: config.task_execution_max_iterations,
      tool_result_model_max_chars: config.tool_result_model_max_chars,
      tool_results_model_total_max_chars: config.tool_results_model_total_max_chars,
    });
  }, [config, form]);

  function handleRuntimeSettingsSubmit(values: RuntimeSettingsFormValues) {
    updateSystemConfigMutation.mutate({
      task_execution_max_iterations: values.task_execution_max_iterations,
      tool_result_model_max_chars: values.tool_result_model_max_chars,
      tool_results_model_total_max_chars: values.tool_results_model_total_max_chars,
    });
  }

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      {contextHolder}
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          {t('settings.title')}
        </Typography.Title>
        <Typography.Text type="secondary">
          {t('settings.subtitle')}
        </Typography.Text>
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
          <Descriptions.Item label="Memory Timeout">
            {config.memory_timeout_ms} ms
          </Descriptions.Item>
          <Descriptions.Item label="Execution Timeout">
            {config.execution_timeout_ms} ms
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
  );
}
