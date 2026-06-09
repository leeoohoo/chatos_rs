import { useQuery } from '@tanstack/react-query';
import {
  Button,
  Collapse,
  Descriptions,
  Divider,
  Empty,
  List,
  Segmented,
  Select,
  Space,
  Statistic,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import { useMemo, useState } from 'react';
import type { ColumnsType } from 'antd/es/table';
import { useNavigate } from 'react-router-dom';

import { api, buildApiUrl } from '../api/client';
import { McpPromptPreviewCard } from '../components/McpPromptPreviewCard';
import type {
  McpCatalogEntry,
  McpServerInfo,
  TaskBuiltinPromptMode,
  TaskMcpInitMode,
} from '../types';

export function McpCatalogPage() {
  const navigate = useNavigate();
  const [mcpEnabled, setMcpEnabled] = useState(true);
  const [initMode, setInitMode] = useState<TaskMcpInitMode>('builtin_only');
  const [promptMode, setPromptMode] = useState<TaskBuiltinPromptMode>('effective');
  const [promptLocale, setPromptLocale] = useState('zh-CN');
  const [selectedKinds, setSelectedKinds] = useState<string[]>([]);
  const serverInfoQuery = useQuery({
    queryKey: ['mcp-server-info'],
    queryFn: api.getMcpServerInfo,
  });
  const catalogQuery = useQuery({
    queryKey: ['mcp-catalog'],
    queryFn: api.listMcpCatalog,
  });
  const remoteServersQuery = useQuery({
    queryKey: ['remote-servers'],
    queryFn: api.listRemoteServers,
  });
  const promptPreviewQuery = useQuery({
    queryKey: ['mcp-prompt-preview', mcpEnabled, initMode, promptMode, promptLocale, selectedKinds],
    queryFn: () =>
      api.previewMcpPrompt({
        enabled: mcpEnabled,
        init_mode: initMode,
        builtin_prompt_mode: promptMode,
        builtin_prompt_locale: promptLocale,
        enabled_builtin_kinds: selectedKinds.length ? selectedKinds : undefined,
      }),
  });
  const kindOptions = useMemo(
    () =>
      (catalogQuery.data || []).map((entry) => ({
        label: entry.kind,
        value: entry.kind,
        disabled: !entry.implemented,
      })),
    [catalogQuery.data],
  );
  const remoteControllerEntry = useMemo(
    () =>
      (catalogQuery.data || []).find((entry) => entry.kind === 'RemoteConnectionController') ||
      null,
    [catalogQuery.data],
  );
  const remoteServerSummary = useMemo(() => {
    const items = remoteServersQuery.data || [];
    return {
      total: items.length,
      enabled: items.filter((item) => item.enabled).length,
      testedSuccess: items.filter((item) => item.last_test_status === 'success').length,
      strict: items.filter((item) => item.host_key_policy === 'strict').length,
    };
  }, [remoteServersQuery.data]);

  const columns: ColumnsType<McpCatalogEntry> = [
    {
      title: 'Builtin Kind',
      dataIndex: 'kind',
      width: 220,
      render: (value: string, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text strong>{value}</Typography.Text>
          <Typography.Text type="secondary">{record.server_name}</Typography.Text>
        </Space>
      ),
    },
    {
      title: '状态',
      dataIndex: 'implemented',
      width: 140,
      render: (implemented: boolean) => (
        <Tag color={implemented ? 'success' : 'warning'}>
          {implemented ? 'implemented' : 'planned'}
        </Tag>
      ),
    },
    {
      title: 'Runtime Default',
      dataIndex: 'runtime_default',
      width: 140,
      render: (runtimeDefault: boolean) =>
        runtimeDefault ? <Tag color="blue">default</Tag> : <Tag>optional</Tag>,
    },
    {
      title: '写权限',
      dataIndex: 'default_allow_writes',
      width: 120,
      render: (allowWrites: boolean) =>
        allowWrites ? <Tag color="volcano">write</Tag> : <Tag color="default">read-only</Tag>,
    },
    {
      title: '已暴露工具数',
      key: 'tool_count',
      width: 140,
      render: (_, record) => record.available_tool_names.length,
    },
    {
      title: '说明',
      dataIndex: 'message',
      render: (message?: string | null) => message || '-',
    },
  ];

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          MCP 目录
        </Typography.Title>
        <Typography.Text type="secondary">
          查看 Task Runner 当前复用了哪些 chatos 内置 builtin MCP，以及对外提供的独立 MCP server 接入信息。
        </Typography.Text>
      </Space>

      {serverInfoQuery.data ? <ExternalMcpServerCard info={serverInfoQuery.data} /> : null}

      <Space
        direction="vertical"
        size="middle"
        style={{
          width: '100%',
          padding: 16,
          borderRadius: 6,
          background: '#fff',
          border: '1px solid #f0f0f0',
        }}
      >
        <Space style={{ justifyContent: 'space-between', width: '100%' }} align="start">
          <Space direction="vertical" size={0}>
            <Typography.Title level={5} style={{ margin: 0 }}>
              RemoteConnectionController
            </Typography.Title>
            <Typography.Text type="secondary">
              这个共享 builtin MCP 现在直接复用 Task Runner 里的服务器清单，不需要 chatos 的远程终端页面。
            </Typography.Text>
          </Space>
          <Space>
            <Button onClick={() => remoteServersQuery.refetch()}>刷新服务器状态</Button>
            <Button type="primary" onClick={() => navigate('/servers')}>
              管理服务器
            </Button>
          </Space>
        </Space>

        <Space size="large" wrap>
          <Statistic title="服务器总数" value={remoteServerSummary.total} />
          <Statistic title="启用中" value={remoteServerSummary.enabled} />
          <Statistic title="测试成功" value={remoteServerSummary.testedSuccess} />
          <Statistic title="严格校验" value={remoteServerSummary.strict} />
        </Space>

        <Descriptions bordered column={1} size="small">
          <Descriptions.Item label="Builtin 状态">
            {remoteControllerEntry ? (
              <Tag color={remoteControllerEntry.implemented ? 'success' : 'warning'}>
                {remoteControllerEntry.implemented ? 'implemented' : 'planned'}
              </Tag>
            ) : (
              '-'
            )}
          </Descriptions.Item>
          <Descriptions.Item label="可用工具数">
            {remoteControllerEntry?.available_tool_names.length ?? 0}
          </Descriptions.Item>
          <Descriptions.Item label="说明">
            {remoteControllerEntry?.message ||
              '服务器来源于“服务器”页面里维护的本地清单，任务执行时会通过共享 RemoteConnectionController 访问。'}
          </Descriptions.Item>
        </Descriptions>
      </Space>

      <Space
        direction="vertical"
        size="middle"
        style={{
          width: '100%',
          padding: 16,
          borderRadius: 6,
          background: '#fff',
          border: '1px solid #f0f0f0',
        }}
      >
        <Space direction="vertical" size={0}>
          <Typography.Title level={5} style={{ margin: 0 }}>
            Builtin Prompt 预览
          </Typography.Title>
          <Typography.Text type="secondary">
            按当前启用状态、初始化模式、语言和 builtin kinds 预览最终注入给模型的 MCP system prompt。
          </Typography.Text>
        </Space>

        <Space wrap>
          <Space size={8}>
            <Typography.Text type="secondary">启用 MCP</Typography.Text>
            <Switch checked={mcpEnabled} onChange={setMcpEnabled} />
          </Space>
          <Select
            style={{ width: 160 }}
            value={initMode}
            onChange={(value) => setInitMode(value)}
            options={[
              { label: 'builtin_only', value: 'builtin_only' },
              { label: 'full', value: 'full' },
              { label: 'disabled', value: 'disabled' },
            ]}
          />
          <Segmented
            value={promptMode}
            onChange={(value) => setPromptMode(value as TaskBuiltinPromptMode)}
            options={[
              { label: 'effective', value: 'effective' },
              { label: 'configured', value: 'configured' },
            ]}
          />
          <Select
            style={{ width: 140 }}
            value={promptLocale}
            onChange={setPromptLocale}
            options={[
              { label: 'zh-CN', value: 'zh-CN' },
              { label: 'en-US', value: 'en-US' },
            ]}
          />
          <Select
            mode="multiple"
            allowClear
            placeholder="默认全部 configurable kinds"
            style={{ minWidth: 320 }}
            value={selectedKinds}
            options={kindOptions}
            onChange={(value) => setSelectedKinds(value)}
          />
        </Space>

        {promptPreviewQuery.data ? (
          <McpPromptPreviewCard preview={promptPreviewQuery.data} />
        ) : null}
      </Space>

      <Table<McpCatalogEntry>
        rowKey="kind"
        loading={catalogQuery.isLoading}
        columns={columns}
        dataSource={catalogQuery.data || []}
        pagination={false}
        locale={{
          emptyText: (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description="当前没有可展示的 MCP builtin 目录"
            />
          ),
        }}
        expandable={{
          expandedRowRender: (record) => (
            <Collapse
              ghost
              items={[
                {
                  key: 'available-tools',
                  label: `可用工具 (${record.available_tool_names.length})`,
                  children: record.available_tool_names.length ? (
                    <List
                      size="small"
                      dataSource={record.available_tool_names}
                      renderItem={(item) => <List.Item>{item}</List.Item>}
                    />
                  ) : (
                    <Typography.Text type="secondary">暂无</Typography.Text>
                  ),
                },
                {
                  key: 'unavailable-tools',
                  label: `不可用工具 (${record.unavailable_tools.length})`,
                  children: record.unavailable_tools.length ? (
                    <List
                      size="small"
                      dataSource={record.unavailable_tools}
                      renderItem={(item) => (
                        <List.Item>
                          <Space direction="vertical" size={0}>
                            <Typography.Text>{item.name}</Typography.Text>
                            <Typography.Text type="secondary">
                              {item.reason}
                            </Typography.Text>
                          </Space>
                        </List.Item>
                      )}
                    />
                  ) : (
                    <Typography.Text type="secondary">暂无</Typography.Text>
                  ),
                },
              ]}
            />
          ),
        }}
      />
    </Space>
  );
}

function ExternalMcpServerCard({ info }: { info: McpServerInfo }) {
  return (
    <Space
      direction="vertical"
      size="middle"
      style={{
        width: '100%',
        padding: 16,
        borderRadius: 6,
        background: '#fff',
        border: '1px solid #f0f0f0',
      }}
    >
      <Space direction="vertical" size={0}>
        <Typography.Title level={5} style={{ margin: 0 }}>
          外部 MCP Server
        </Typography.Title>
        <Typography.Text type="secondary">
          其他系统可以通过这个 HTTP JSON-RPC 入口调用 Task Runner 的任务、运行和人工提示能力。
        </Typography.Text>
      </Space>

      <Space wrap>
        {info.transports.map((transport) => (
          <Tag key={transport} color="blue">
            {transport}
          </Tag>
        ))}
      </Space>

      <Divider style={{ margin: '4px 0' }} />

      {info.http_endpoint_path ? (
        <Space direction="vertical" size={4}>
          <Typography.Text strong>HTTP Endpoint</Typography.Text>
          <Typography.Text code>{buildApiUrl(info.http_endpoint_path)}</Typography.Text>
        </Space>
      ) : null}

      {info.stdio_command ? (
        <Space direction="vertical" size={4}>
          <Typography.Text strong>stdio Command</Typography.Text>
          <Typography.Text code>
            {[info.stdio_command, ...info.stdio_args].join(' ')}
          </Typography.Text>
        </Space>
      ) : null}

      <Divider style={{ margin: '4px 0' }} />

      <Space direction="vertical" size={4}>
        <Typography.Text strong>已暴露工具</Typography.Text>
        <List
          size="small"
          bordered
          dataSource={info.tool_names}
          renderItem={(item) => <List.Item>{item}</List.Item>}
        />
      </Space>
    </Space>
  );
}
