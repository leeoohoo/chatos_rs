// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { EditOutlined, SettingOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Alert,
  Button,
  Input,
  Modal,
  Segmented,
  Space,
  Table,
  Tag,
  Tooltip,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useEffect, useMemo, useState } from 'react';

import { api } from '../api/client';
import { EnabledTag, RuntimeKindTag, VisibilityTag } from '../components/Tags';
import { useI18n } from '../i18n/I18nProvider';
import { agentDisplayName, mcpDisplayName } from '../i18n/labels';
import { agentPromptVendorLabel } from './agentPrompts/support';
import type {
  AgentPromptCompleteness,
  AgentMcpBindingView,
  CurrentUser,
  McpBindingMode,
  SystemAgentRecord,
} from '../types';

interface SystemAgentsPageProps {
  user: CurrentUser;
  onOpenPromptSettings: (agentKey: string) => void;
}

export function SystemAgentsPage({ user, onOpenPromptSettings }: SystemAgentsPageProps) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [selectedAgentKey, setSelectedAgentKey] = useState<string | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [modes, setModes] = useState<Record<string, McpBindingMode>>({});
  const isAdmin = user.role === 'super_admin';

  const agentsQuery = useQuery({
    queryKey: ['system-agents'],
    queryFn: api.listSystemAgents,
    enabled: isAdmin,
  });

  const completenessQuery = useQuery({
    queryKey: ['agent-prompt-completeness'],
    queryFn: api.agentPromptCompleteness,
    enabled: isAdmin,
  });
  const completeness = useMemo(
    () => new Map((completenessQuery.data || []).map((item) => [item.agent_key, item])),
    [completenessQuery.data],
  );

  const bindingsQuery = useQuery({
    queryKey: ['agent-mcp-bindings', selectedAgentKey],
    queryFn: () => api.getAgentMcpBindings(selectedAgentKey || ''),
    enabled: isAdmin && modalOpen && Boolean(selectedAgentKey),
  });

  useEffect(() => {
    if (!bindingsQuery.data) {
      return;
    }
    setModes(
      Object.fromEntries(bindingsQuery.data.items.map((item) => [item.mcp.id, item.mode])),
    );
  }, [bindingsQuery.data]);

  const saveMutation = useMutation({
    mutationFn: () => {
      return api.updateAgentMcpBindings(
        selectedAgentKey || '',
        Object.entries(modes)
          .filter(([, mode]) => mode !== 'disabled')
          .map(([mcp_id, mode]) => ({ mcp_id, mode })),
      );
    },
    onSuccess: (data) => {
      message.success(t('agent.mcpConfigSaved'));
      queryClient.setQueryData(['agent-mcp-bindings', selectedAgentKey], data);
      setModalOpen(false);
    },
    onError: (error) => message.error((error as Error).message),
  });

  const agentColumns = useMemo<ColumnsType<SystemAgentRecord>>(
    () => [
      {
        title: t('agent.title'),
        dataIndex: 'display_name',
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text strong>{agentDisplayName(record, t)}</Typography.Text>
            <Typography.Text type="secondary">{record.agent_key}</Typography.Text>
          </Space>
        ),
      },
      { title: t('table.service'), dataIndex: 'service_name', width: 190 },
      {
        title: t('agent.toolPlane'),
        dataIndex: 'tool_plane',
        width: 125,
        render: (toolPlane: SystemAgentRecord['tool_plane']) => (
          <Tag color={toolPlane === 'managed' ? 'blue' : 'default'}>
            {t(`agent.toolPlane.${toolPlane}`)}
          </Tag>
        ),
      },
      {
        title: t('table.status'),
        dataIndex: 'enabled',
        width: 110,
        render: (enabled) => <EnabledTag enabled={enabled} />,
      },
      {
        title: t('agent.promptStatus'),
        key: 'prompt_status',
        width: 130,
        render: (_, record) => {
          const item = completeness.get(record.agent_key) as AgentPromptCompleteness | undefined;
          const published = item?.published_vendors.map(agentPromptVendorLabel).join(' / ') || '-';
          const missing = item?.missing_vendors.map(agentPromptVendorLabel).join(' / ') || '-';
          return (
            <Tooltip
              title={t('agent.promptStatusTooltip', {
                published,
                missing,
              })}
            >
              <Typography.Text type={item?.ready ? 'success' : 'warning'}>
                {t('agent.promptCount', { count: item?.published_vendors.length || 0 })}
              </Typography.Text>
            </Tooltip>
          );
        },
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 260,
        render: (_, record) => (
          <Space>
            <Button
              icon={<SettingOutlined />}
              disabled={record.tool_plane === 'none'}
              title={record.tool_plane === 'none' ? t('agent.toolPlaneNoneNotice') : undefined}
              onClick={() => {
                setSelectedAgentKey(record.agent_key);
                setSearch('');
                setModes({});
                setModalOpen(true);
              }}
            >
              {t('agent.configureMcp')}
            </Button>
            <Button
              icon={<EditOutlined />}
              onClick={() => onOpenPromptSettings(record.agent_key)}
            >
              {t('agent.promptSettings')}
            </Button>
          </Space>
        ),
      },
    ],
    [completeness, onOpenPromptSettings, t],
  );

  const mcpItems = useMemo(() => {
    const items = bindingsQuery.data?.items || [];
    const keyword = search.trim().toLowerCase();
    if (!keyword) {
      return items;
    }
    return items.filter((item) =>
      [
        item.mcp.id,
        item.mcp.name,
        item.mcp.display_name,
        item.mcp.description,
        item.mcp.visibility,
        item.mcp.source_kind,
        item.mcp.owner_kind,
        item.mcp.owner_user_id,
        item.mcp.runtime.kind,
        item.mcp.runtime.builtin_kind,
        item.mcp.runtime.system_key,
        item.mcp.runtime.server_name,
        item.mcp.plugin_id,
        item.mcp.component_key,
      ]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(keyword)),
    );
  }, [bindingsQuery.data, search]);

  const mcpStats = useMemo(() => {
    const items = bindingsQuery.data?.items || [];
    return {
      total: items.length,
      shown: mcpItems.length,
      bound: Object.values(modes).filter((mode) => mode !== 'disabled').length,
    };
  }, [bindingsQuery.data, mcpItems.length, modes]);

  const mcpColumns = useMemo<ColumnsType<AgentMcpBindingView>>(
    () => [
      {
        title: t('table.name'),
        dataIndex: ['mcp', 'display_name'],
        render: (_, item) => (
          <Space direction="vertical" size={0}>
            <Space size={6} wrap>
              <Typography.Text strong>{mcpDisplayName(item.mcp, t)}</Typography.Text>
            </Space>
            <Typography.Text type="secondary">
              {item.mcp.name} · {item.mcp.id}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: t('table.source'),
        key: 'source',
        width: 290,
        render: (_, item) => (
          <Space direction="vertical" size={2}>
            <Space size={4} wrap>
              <VisibilityTag value={item.mcp.visibility} />
              <RuntimeKindTag value={item.mcp.runtime.kind} />
            </Space>
            <Typography.Text type="secondary">
              {sourceKindLabel(item.mcp.source_kind, t)}
              {item.mcp.plugin_id ? ` · ${item.mcp.plugin_id}` : ''}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: t('table.status'),
        dataIndex: ['mcp', 'enabled'],
        width: 145,
        render: (enabled, item) => (
          <Space direction="vertical" size={2}>
            <EnabledTag enabled={enabled} />
            <Tag color={item.mode === 'disabled' ? 'default' : 'green'}>
              {item.mode === 'disabled' ? t('agent.mcpNotBound') : t('agent.mcpBoundToRuntime')}
            </Tag>
          </Space>
        ),
      },
      {
        title: t('agent.mcpMode'),
        key: 'mode',
        width: 310,
        render: (_, item) => (
          <Space direction="vertical" size={4} className="full-width">
            <Segmented
              className="mcp-mode-control"
              block
              value={modes[item.mcp.id] || 'disabled'}
              options={[
                { value: 'disabled', label: t('mcpMode.disabled') },
                { value: 'optional', label: t('mcpMode.optional') },
                { value: 'required', label: t('mcpMode.required') },
              ]}
              onChange={(value) =>
                setModes((current) => ({
                  ...current,
                  [item.mcp.id]: value as McpBindingMode,
                }))
              }
            />
          </Space>
        ),
      },
    ],
    [modes, t],
  );

  const selectedAgent = agentsQuery.data?.find((agent) => agent.agent_key === selectedAgentKey);

  if (!isAdmin) {
    return <Alert type="error" showIcon message={t('admin.only')} />;
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('agent.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('agent.descriptionSimple')}</Typography.Text>
        </Space>
      </div>
      <Table
        rowKey="agent_key"
        columns={agentColumns}
        dataSource={agentsQuery.data || []}
        loading={agentsQuery.isLoading}
        tableLayout="fixed"
        pagination={false}
      />
      <Modal
        title={
          selectedAgent
            ? `${agentDisplayName(selectedAgent, t)} · ${t('agent.configureMcp')}`
            : t('agent.configureMcp')
        }
        open={modalOpen}
        width={1180}
        onCancel={() => setModalOpen(false)}
        onOk={() => saveMutation.mutate()}
        confirmLoading={saveMutation.isPending}
        destroyOnClose
      >
        <Alert
          className="mcp-binding-notice"
          type="info"
          showIcon
          message={t('agent.mcpCatalogNotice')}
        />
        <Typography.Text type="secondary">
          {t('agent.mcpCatalogStats', mcpStats)}
        </Typography.Text>
        <Input.Search
          className="mcp-binding-search"
          allowClear
          value={search}
          placeholder={t('agent.searchMcp')}
          onChange={(event) => setSearch(event.target.value)}
        />
        <Table
          rowKey={(item) => item.mcp.id}
          className="mcp-binding-table"
          columns={mcpColumns}
          dataSource={mcpItems}
          loading={bindingsQuery.isLoading}
          pagination={false}
          tableLayout="fixed"
          scroll={{ y: 520 }}
        />
      </Modal>
    </div>
  );
}

function sourceKindLabel(value: string, t: (key: string) => string): string {
  const key = `sourceKind.${value}`;
  const translated = t(key);
  return translated === key ? value : translated;
}
