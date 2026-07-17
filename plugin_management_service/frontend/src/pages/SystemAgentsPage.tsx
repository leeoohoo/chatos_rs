// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { EditOutlined, SettingOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Input, Modal, Segmented, Space, Table, Typography, message } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useEffect, useMemo, useState } from 'react';

import { api } from '../api/client';
import { EnabledTag } from '../components/Tags';
import { useI18n } from '../i18n/I18nProvider';
import { agentDisplayName, mcpDisplayName } from '../i18n/labels';
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
    mutationFn: () =>
      api.updateAgentMcpBindings(
        selectedAgentKey || '',
        Object.entries(modes).map(([mcp_id, mode]) => ({ mcp_id, mode })),
      ),
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
          return (
            <Typography.Text type={item?.ready ? 'success' : 'warning'}>
              {t('agent.promptCount', { count: item?.published_vendors.length || 0 })}
            </Typography.Text>
          );
        },
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 260,
        render: (_, record) => {
          const supportsMcp = record.service_name !== 'memory-engine';
          return (
            <Space>
              {supportsMcp ? (
                <Button
                  icon={<SettingOutlined />}
                  onClick={() => {
                    setSelectedAgentKey(record.agent_key);
                    setSearch('');
                    setModalOpen(true);
                  }}
                >
                  {t('agent.configureMcp')}
                </Button>
              ) : null}
              <Button
                icon={<EditOutlined />}
                onClick={() => onOpenPromptSettings(record.agent_key)}
              >
                {t('agent.promptSettings')}
              </Button>
            </Space>
          );
        },
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
      [item.mcp.name, item.mcp.display_name, item.mcp.runtime.builtin_kind]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(keyword)),
    );
  }, [bindingsQuery.data, search]);

  const mcpColumns = useMemo<ColumnsType<AgentMcpBindingView>>(
    () => [
      {
        title: t('table.name'),
        dataIndex: ['mcp', 'display_name'],
        render: (_, item) => (
          <Space direction="vertical" size={0}>
            <Typography.Text strong>{mcpDisplayName(item.mcp, t)}</Typography.Text>
            <Typography.Text type="secondary">{item.mcp.name}</Typography.Text>
          </Space>
        ),
      },
      {
        title: t('table.status'),
        dataIndex: ['mcp', 'enabled'],
        width: 100,
        render: (enabled) => <EnabledTag enabled={enabled} />,
      },
      {
        title: t('agent.mcpMode'),
        key: 'mode',
        width: 310,
        render: (_, item) => (
          <Segmented
            className="mcp-mode-control"
            block
            disabled={!item.mcp.enabled}
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
        width={820}
        onCancel={() => setModalOpen(false)}
        onOk={() => saveMutation.mutate()}
        confirmLoading={saveMutation.isPending}
        destroyOnClose
      >
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
          scroll={{ y: 330 }}
        />
      </Modal>
    </div>
  );
}
