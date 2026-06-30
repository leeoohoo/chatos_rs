import {
  ApiOutlined,
  FileTextOutlined,
  PlayCircleOutlined,
  ReloadOutlined,
  SafetyCertificateOutlined,
} from '@ant-design/icons';
import {
  Alert,
  Button,
  Col,
  Descriptions,
  Form,
  Input,
  Row,
  Select,
  Space,
  Table,
  Tag,
  Typography,
  message,
} from 'antd';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';

import { sandboxesApi } from '../api/sandboxes';
import { StatusTag } from '../components/StatusTag';
import { useI18n } from '../i18n';
import type { SandboxHealthResponse, SandboxLeaseRecord } from '../types';

const { TextArea } = Input;

interface McpCallFormValues {
  name: string;
  arguments: string;
}

interface McpToolRow {
  key: string;
  name: string;
  description: string;
  raw: unknown;
}

const defaultArguments = JSON.stringify({ path: '.', common: 'pwd && ls -la' }, null, 2);

export function McpTestPage() {
  const { t } = useI18n();
  const [form] = Form.useForm<McpCallFormValues>();
  const [selectedSandboxId, setSelectedSandboxId] = useState<string>();
  const [health, setHealth] = useState<SandboxHealthResponse>();
  const [tools, setTools] = useState<unknown[]>([]);
  const [callResult, setCallResult] = useState<unknown>();

  const sandboxesQuery = useQuery({
    queryKey: ['sandboxes', 'mcp-test'],
    queryFn: () => sandboxesApi.list(),
  });

  const sandboxes = sandboxesQuery.data ?? [];
  const selectedSandbox = sandboxes.find((item) => item.sandbox_id === selectedSandboxId);
  const sandboxOptions = useMemo(
    () =>
      sandboxes.map((sandbox) => ({
        label: `${sandbox.sandbox_id} · ${sandbox.status} · ${sandbox.project_id}`,
        value: sandbox.sandbox_id,
      })),
    [sandboxes],
  );
  const toolRows = useMemo(() => toToolRows(tools), [tools]);

  useEffect(() => {
    if (!sandboxes.length) {
      return;
    }
    if (selectedSandboxId && sandboxes.some((item) => item.sandbox_id === selectedSandboxId)) {
      return;
    }
    const preferred =
      sandboxes.find((item) => item.status === 'ready' || item.status === 'running') ?? sandboxes[0];
    setSelectedSandboxId(preferred.sandbox_id);
  }, [sandboxes, selectedSandboxId]);

  useEffect(() => {
    setHealth(undefined);
    setTools([]);
    setCallResult(undefined);
  }, [selectedSandboxId]);

  const healthMutation = useMutation({
    mutationFn: (sandboxId: string) => sandboxesApi.health(sandboxId),
    onSuccess: (result) => {
      setHealth(result);
      if (result.ok) {
        message.success(t('mcp.healthSuccess'));
      } else {
        message.warning(t('mcp.healthWarning'));
      }
    },
    onError: (error) => {
      message.error(error instanceof Error ? error.message : t('mcp.healthFailure'));
    },
  });

  const toolsMutation = useMutation({
    mutationFn: (sandboxId: string) => sandboxesApi.mcpTools(sandboxId),
    onSuccess: (result) => {
      setTools(result.tools);
      const rows = toToolRows(result.tools);
      const preferred = rows.find((row) => row.name === 'execute_command') ?? rows[0];
      if (preferred) {
        form.setFieldsValue({
          name: preferred.name,
          arguments: preferred.name === 'execute_command' ? defaultArguments : '{}',
        });
      }
      message.success(t('mcp.toolsSuccess'));
    },
    onError: (error) => {
      message.error(error instanceof Error ? error.message : t('mcp.toolsFailure'));
    },
  });

  const callMutation = useMutation({
    mutationFn: ({
      sandboxId,
      name,
      argumentsValue,
    }: {
      sandboxId: string;
      name: string;
      argumentsValue: unknown;
    }) => sandboxesApi.mcpCall(sandboxId, { name, arguments: argumentsValue }),
    onSuccess: (result) => {
      setCallResult(result);
      message.success(t('mcp.callSuccess'));
    },
    onError: (error) => {
      message.error(error instanceof Error ? error.message : t('mcp.callFailure'));
    },
  });

  const requireSandboxId = () => {
    if (!selectedSandboxId) {
      message.warning(t('mcp.selectSandboxFirst'));
      return undefined;
    }
    return selectedSandboxId;
  };

  const applyExample = (name: string, value: unknown) => {
    form.setFieldsValue({ name, arguments: JSON.stringify(value, null, 2) });
    setCallResult(undefined);
  };

  const handleCall = async () => {
    const sandboxId = requireSandboxId();
    if (!sandboxId) {
      return;
    }
    const values = await form.validateFields();
    let parsed: unknown;
    try {
      parsed = values.arguments.trim() ? JSON.parse(values.arguments) : {};
    } catch (error) {
      message.error(error instanceof Error ? error.message : t('mcp.invalidJson'));
      return;
    }
    callMutation.mutate({
      sandboxId,
      name: values.name,
      argumentsValue: parsed,
    });
  };

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <div className="page-heading">
        <div>
          <Typography.Title level={3}>{t('mcp.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('mcp.subtitle')}</Typography.Text>
        </div>
        <Button icon={<ReloadOutlined />} onClick={() => void sandboxesQuery.refetch()}>
          {t('common.refresh')}
        </Button>
      </div>

      <div className="surface">
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <Row gutter={12}>
            <Col xs={24} lg={12}>
              <Space direction="vertical" size={8} style={{ width: '100%' }}>
                <Typography.Text strong>{t('mcp.targetSandbox')}</Typography.Text>
                <Select
                  showSearch
                  value={selectedSandboxId}
                  loading={sandboxesQuery.isLoading}
                  options={sandboxOptions}
                  optionFilterProp="label"
                  style={{ width: '100%' }}
                  onChange={setSelectedSandboxId}
                />
              </Space>
            </Col>
            <Col xs={24} lg={12}>
              <Space wrap style={{ marginTop: 26 }}>
                <Button
                  icon={<SafetyCertificateOutlined />}
                  loading={healthMutation.isPending}
                  disabled={!selectedSandboxId}
                  onClick={() => {
                    const sandboxId = requireSandboxId();
                    if (sandboxId) {
                      healthMutation.mutate(sandboxId);
                    }
                  }}
                >
                  {t('mcp.runHealth')}
                </Button>
                <Button
                  type="primary"
                  icon={<ApiOutlined />}
                  loading={toolsMutation.isPending}
                  disabled={!selectedSandboxId}
                  onClick={() => {
                    const sandboxId = requireSandboxId();
                    if (sandboxId) {
                      toolsMutation.mutate(sandboxId);
                    }
                  }}
                >
                  {t('mcp.loadTools')}
                </Button>
              </Space>
            </Col>
          </Row>

          <Descriptions bordered column={2}>
            <Descriptions.Item label={t('common.status')}>
              {selectedSandbox ? <StatusTag status={selectedSandbox.status} /> : '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('common.backend')}>
              {selectedSandbox?.backend ?? '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('common.project')}>
              {selectedSandbox?.project_id ?? '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('common.run')}>
              {selectedSandbox?.run_id ?? '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('common.agent')} span={2}>
              <Typography.Text copyable>{selectedSandbox?.agent_endpoint ?? '-'}</Typography.Text>
            </Descriptions.Item>
          </Descriptions>

          {health ? (
            <Alert
              showIcon
              type={health.ok ? 'success' : 'warning'}
              message={health.ok ? t('health.healthy') : t('health.unhealthy')}
              description={health.message}
            />
          ) : null}
        </Space>
      </div>

      <div className="surface">
        <Space direction="vertical" size={14} style={{ width: '100%' }}>
          <div className="section-heading-row">
            <Typography.Title level={4}>{t('mcp.tools')}</Typography.Title>
            <Tag color={toolRows.length ? 'blue' : 'default'}>
              {toolRows.length} {t('mcp.toolsUnit')}
            </Tag>
          </div>
          <Table<McpToolRow>
            size="small"
            rowKey="key"
            dataSource={toolRows}
            pagination={{ pageSize: 8 }}
            scroll={{ x: 820 }}
            columns={[
              {
                title: t('mcp.toolName'),
                dataIndex: 'name',
                width: 260,
                render: (name) => <Typography.Text code>{name}</Typography.Text>,
              },
              {
                title: t('mcp.description'),
                dataIndex: 'description',
                ellipsis: true,
              },
            ]}
          />
        </Space>
      </div>

      <div className="surface">
        <Space direction="vertical" size={14} style={{ width: '100%' }}>
          <Typography.Title level={4}>{t('mcp.callTool')}</Typography.Title>
          <Space wrap>
            <Button
              icon={<PlayCircleOutlined />}
              onClick={() =>
                applyExample('execute_command', {
                  path: '.',
                  common: 'pwd && ls -la',
                })
              }
            >
              {t('mcp.exampleCommand')}
            </Button>
            <Button
              icon={<FileTextOutlined />}
              onClick={() =>
                applyExample('write_file', {
                  path: 'mcp-test.txt',
                  content: 'hello from sandbox mcp\n',
                })
              }
            >
              {t('mcp.exampleWrite')}
            </Button>
            <Button
              icon={<FileTextOutlined />}
              onClick={() =>
                applyExample('read_file_raw', {
                  path: 'mcp-test.txt',
                  with_line_numbers: false,
                })
              }
            >
              {t('mcp.exampleRead')}
            </Button>
          </Space>
          <Form
            form={form}
            layout="vertical"
            initialValues={{ name: 'execute_command', arguments: defaultArguments }}
          >
            <Form.Item name="name" label={t('mcp.toolName')} rules={[{ required: true }]}>
              <Select
                showSearch
                optionFilterProp="label"
                options={toolRows.map((tool) => ({ label: tool.name, value: tool.name }))}
              />
            </Form.Item>
            <Form.Item name="arguments" label={t('mcp.arguments')} rules={[{ required: true }]}>
              <TextArea className="json-input" spellCheck={false} autoSize={{ minRows: 8, maxRows: 14 }} />
            </Form.Item>
            <Button
              type="primary"
              icon={<PlayCircleOutlined />}
              loading={callMutation.isPending}
              disabled={!selectedSandboxId}
              onClick={() => void handleCall()}
            >
              {t('mcp.call')}
            </Button>
          </Form>
        </Space>
      </div>

      {callResult ? (
        <div className="surface">
          <Typography.Title level={4}>{t('mcp.result')}</Typography.Title>
          <pre className="json-panel">{JSON.stringify(callResult, null, 2)}</pre>
        </div>
      ) : null}
    </Space>
  );
}

function toToolRows(tools: unknown[]): McpToolRow[] {
  return tools
    .map((tool, index) => {
      const item = isRecord(tool) ? tool : {};
      const name = typeof item.name === 'string' ? item.name : `tool_${index + 1}`;
      const description = typeof item.description === 'string' ? item.description : '';
      return {
        key: name,
        name,
        description,
        raw: tool,
      };
    })
    .sort((left, right) => left.name.localeCompare(right.name));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
