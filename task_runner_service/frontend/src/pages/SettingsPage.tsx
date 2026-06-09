import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { Alert, Button, Descriptions, Space, Statistic, Tag, Typography } from 'antd';
import dayjs from 'dayjs';

import { api } from '../api/client';

export function SettingsPage() {
  const navigate = useNavigate();
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

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          设置
        </Typography.Title>
        <Typography.Text type="secondary">
          查看 Task Runner 当前运行时配置、存储模式和 Memory Engine 接线状态。
        </Typography.Text>
      </Space>

      {health ? (
        <Alert
          type={health.status === 'ok' ? 'success' : 'warning'}
          message={`${health.service} / ${health.status}`}
          description={`最后探测时间: ${dayjs(health.now).format('YYYY-MM-DD HH:mm:ss')}`}
          showIcon
        />
      ) : null}

      <Space size="large" wrap>
        <Statistic title="Builtin MCP" value={implementedBuiltinCount} />
        <Statistic title="Runtime Default" value={runtimeDefaultCount} />
        <Statistic title="对外暴露工具" value={mcpServer?.tool_names.length || 0} />
      </Space>

      {config ? (
        <Descriptions bordered column={1} size="small">
          <Descriptions.Item label="HTTP 监听">
            {config.host}:{config.port}
          </Descriptions.Item>
          <Descriptions.Item label="存储模式">
            <Tag color={storeModeColor}>{config.store_mode}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="数据库">{config.database_url}</Descriptions.Item>
          <Descriptions.Item label="Memory Engine">
            <Tag color={config.memory_engine_configured ? 'success' : 'default'}>
              {config.memory_engine_configured ? 'configured' : 'not configured'}
            </Tag>
          </Descriptions.Item>
          <Descriptions.Item label="Memory Base URL">
            {config.memory_engine_base_url || '-'}
          </Descriptions.Item>
          <Descriptions.Item label="Memory Source ID">
            {config.memory_engine_source_id}
          </Descriptions.Item>
          <Descriptions.Item label="默认 Tenant">
            {config.default_tenant_id}
          </Descriptions.Item>
          <Descriptions.Item label="默认 Subject">
            {config.default_subject_id}
          </Descriptions.Item>
          <Descriptions.Item label="默认 Workspace">
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
        </Descriptions>
      ) : null}

      {mcpServer ? (
        <Descriptions
          title="MCP 服务"
          bordered
          column={1}
          size="small"
          extra={
            <Button size="small" onClick={() => navigate('/mcp')}>
              打开 MCP 目录
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
          <Descriptions.Item label="暴露工具数">
            {mcpServer.tool_names.length}
          </Descriptions.Item>
        </Descriptions>
      ) : null}
    </Space>
  );
}
