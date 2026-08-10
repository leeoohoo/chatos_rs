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
  BindingConditions,
  CurrentUser,
  McpBindingMode,
  SystemAgentRecord,
} from '../types';

interface SystemAgentsPageProps {
  user: CurrentUser;
  onOpenPromptSettings: (agentKey: string) => void;
}

interface McpBindingDraft {
  mcp_id: string;
  mode: McpBindingMode;
  variants: Array<{
    binding_id?: string | null;
    conditions: BindingConditions;
    tool_allowlist: string[];
    tool_blocklist: string[];
  }>;
}

interface MergedMcpBindingItem {
  mcp: AgentMcpBindingView['mcp'];
  mode: McpBindingMode;
  bindable: boolean;
  unavailable_reason?: string | null;
  variants: Array<{
    binding_id?: string | null;
    conditions: BindingConditions;
    tool_allowlist: string[];
    tool_blocklist: string[];
  }>;
}

export function SystemAgentsPage({ user, onOpenPromptSettings }: SystemAgentsPageProps) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [selectedAgentKey, setSelectedAgentKey] = useState<string | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [bindingDrafts, setBindingDrafts] = useState<Record<string, McpBindingDraft>>({});
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

  const mergedBindingItems = useMemo(
    () => mergeAgentMcpBindings(bindingsQuery.data?.items || []),
    [bindingsQuery.data],
  );

  useEffect(() => {
    if (!mergedBindingItems.length) {
      return;
    }
    setBindingDrafts(
      Object.fromEntries(
        mergedBindingItems.map((item) => [
          bindingRowKey(item),
          {
            mcp_id: item.mcp.id,
            mode: item.mode,
            variants: item.variants,
          },
        ]),
      ),
    );
  }, [mergedBindingItems]);

  const saveMutation = useMutation({
    mutationFn: () => {
      return api.updateAgentMcpBindings(
        selectedAgentKey || '',
        Object.values(bindingDrafts)
          .flatMap((binding) =>
            binding.variants.map((variant) => ({
              mcp_id: binding.mcp_id,
              mode: binding.mode,
              conditions: variant.conditions,
              tool_allowlist: variant.tool_allowlist,
              tool_blocklist: variant.tool_blocklist,
            })),
          ),
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
          <Tag
            color={
              toolPlane === 'managed' ? 'blue' : toolPlane === 'local_only' ? 'cyan' : 'default'
            }
          >
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
              disabled={record.tool_plane !== 'managed'}
              title={
                record.tool_plane === 'local_only'
                  ? t('agent.toolPlaneLocalOnlyNotice')
                  : record.tool_plane === 'none'
                    ? t('agent.toolPlaneNoneNotice')
                    : undefined
              }
              onClick={() => {
                setSelectedAgentKey(record.agent_key);
                setSearch('');
                setBindingDrafts({});
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
    const items = mergedBindingItems;
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
        ...item.variants.flatMap((variant) => [
          variant.conditions.task_profile,
          variant.conditions.project_source_type,
          variant.conditions.schedule_mode,
          variant.tool_allowlist.join(' '),
          variant.tool_blocklist.join(' '),
        ]),
      ]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(keyword)),
    );
  }, [mergedBindingItems, search]);

  const mcpStats = useMemo(() => {
    const items = mergedBindingItems;
    return {
      total: items.length,
      shown: mcpItems.length,
      bound: Object.values(bindingDrafts).filter((binding) => binding.mode !== 'disabled').length,
    };
  }, [bindingDrafts, mergedBindingItems, mcpItems.length]);

  const mcpColumns = useMemo<ColumnsType<MergedMcpBindingItem>>(
    () => [
      {
        title: t('table.name'),
        dataIndex: ['mcp', 'display_name'],
        width: 300,
        render: (_, item) => (
          <Space direction="vertical" size={2} className="mcp-binding-name">
            <Space size={6} wrap>
              <Typography.Text strong>{mcpDisplayName(item.mcp, t)}</Typography.Text>
            </Space>
            <Typography.Text type="secondary" className="mcp-binding-meta">
              {item.mcp.name} · {item.mcp.id}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: t('table.source'),
        key: 'source',
        width: 180,
        render: (_, item) => (
          <Space direction="vertical" size={6}>
            <Space size={4} wrap>
              <VisibilityTag value={item.mcp.visibility} />
              <RuntimeKindTag value={item.mcp.runtime.kind} />
            </Space>
            <Typography.Text type="secondary" className="mcp-binding-meta">
              {sourceKindLabel(item.mcp.source_kind, t)}
              {item.mcp.plugin_id ? ` · ${item.mcp.plugin_id}` : ''}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: t('agent.mcpConditions'),
        key: 'conditions',
        width: 180,
        render: (_, item) => (
          <Space size={[4, 4]} wrap>
            {bindingConditionEntries(item).length > 0 ? (
              bindingConditionEntries(item).map(([label, value]) => (
                <Tag key={`${label}:${value}`}>{`${label}=${value}`}</Tag>
              ))
            ) : (
              <Typography.Text type="secondary">{t('agent.mcpDefaultRule')}</Typography.Text>
            )}
          </Space>
        ),
      },
      {
        title: t('agent.mcpToolPolicy'),
        key: 'tool_policy',
        width: 230,
        render: (_, item) => (
          <Space direction="vertical" size={6} className="mcp-binding-policy">
            <Space size={4} wrap>
              {allVariantsUseAllowlist(item) ? (
                <Tag color="blue">
                  {t('agent.mcpAllowlistCount', { count: item.variants[0]?.tool_allowlist.length || 0 })}
                </Tag>
              ) : (
                <Tag>{t('agent.mcpAllTools')}</Tag>
              )}
              {item.variants.some((variant) => variant.tool_blocklist.length > 0) ? (
                <Tag color="orange">
                  {t('agent.mcpBlocklistCount', {
                    count: Math.max(...item.variants.map((variant) => variant.tool_blocklist.length)),
                  })}
                </Tag>
              ) : null}
            </Space>
            <Typography.Text type="secondary" className="mcp-binding-meta">
              {toolPolicyPreview(item)}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: t('table.status'),
        dataIndex: ['mcp', 'enabled'],
        width: 145,
        render: (enabled, item) => (
          <Space direction="vertical" size={6} className="mcp-binding-status">
            <EnabledTag enabled={enabled} />
            <Tag
              color={
                (bindingDrafts[bindingRowKey(item)]?.mode || item.mode) === 'disabled'
                  ? 'default'
                  : 'green'
              }
            >
              {(bindingDrafts[bindingRowKey(item)]?.mode || item.mode) === 'disabled'
                ? t('agent.mcpNotBound')
                : t('agent.mcpBoundToRuntime')}
            </Tag>
          </Space>
        ),
      },
      {
        title: t('agent.mcpMode'),
        key: 'mode',
        width: 210,
        render: (_, item) => (
          <div className="mcp-mode-cell">
            <Segmented
              className="mcp-mode-control"
              block
              value={bindingDrafts[bindingRowKey(item)]?.mode || 'disabled'}
              options={[
                { value: 'disabled', label: t('mcpMode.disabled') },
                { value: 'optional', label: t('mcpMode.optional') },
                { value: 'required', label: t('mcpMode.required') },
              ]}
              onChange={(value) =>
                setBindingDrafts((current) => ({
                  ...current,
                  [bindingRowKey(item)]: {
                    ...(current[bindingRowKey(item)] || {
                      mcp_id: item.mcp.id,
                      variants: item.variants,
                    }),
                    mode: value as McpBindingMode,
                  },
                }))
              }
            />
          </div>
        ),
      },
    ],
    [bindingDrafts, t],
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
        className="agent-mcp-modal"
        title={
          selectedAgent
            ? `${agentDisplayName(selectedAgent, t)} · ${t('agent.configureMcp')}`
            : t('agent.configureMcp')
        }
        open={modalOpen}
        width="min(1480px, calc(100vw - 48px))"
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
        <div className="mcp-binding-toolbar">
          <Typography.Text type="secondary" className="mcp-binding-stats">
            {t('agent.mcpCatalogStats', mcpStats)}
          </Typography.Text>
          <Input.Search
            className="mcp-binding-search"
            allowClear
            value={search}
            placeholder={t('agent.searchMcp')}
            onChange={(event) => setSearch(event.target.value)}
          />
        </div>
        <div className="mcp-binding-table-shell">
          <Table
            rowKey={(item) => bindingRowKey(item)}
            className="mcp-binding-table"
            columns={mcpColumns}
            dataSource={mcpItems}
            loading={bindingsQuery.isLoading}
            pagination={false}
            size="middle"
            tableLayout="fixed"
            scroll={
              mcpItems.length > 6
                ? { x: 1450, y: 440 }
                : { x: 1450 }
            }
          />
        </div>
      </Modal>
    </div>
  );
}

function bindingRowKey(item: MergedMcpBindingItem): string {
  return item.mcp.id;
}

function mergeAgentMcpBindings(items: AgentMcpBindingView[]): MergedMcpBindingItem[] {
  const grouped = new Map<string, MergedMcpBindingItem>();
  for (const item of items) {
    const existing = grouped.get(item.mcp.id);
    const variant = {
      binding_id: item.binding_id,
      conditions: item.conditions,
      tool_allowlist: item.tool_allowlist,
      tool_blocklist: item.tool_blocklist,
    };
    if (existing) {
      existing.variants.push(variant);
      existing.mode = mergeBindingModes(existing.mode, item.mode);
      existing.bindable = existing.bindable && item.bindable;
      existing.unavailable_reason = existing.unavailable_reason || item.unavailable_reason;
      continue;
    }
    grouped.set(item.mcp.id, {
      mcp: item.mcp,
      mode: item.mode,
      bindable: item.bindable,
      unavailable_reason: item.unavailable_reason,
      variants: [variant],
    });
  }
  return Array.from(grouped.values());
}

function mergeBindingModes(left: McpBindingMode, right: McpBindingMode): McpBindingMode {
  if (left === right) {
    return left;
  }
  if (left === 'disabled' && right === 'disabled') {
    return 'disabled';
  }
  if (left === 'required' && right === 'required') {
    return 'required';
  }
  return 'optional';
}

function bindingConditionEntries(item: MergedMcpBindingItem): Array<[string, string]> {
  const entries: Array<[string, string]> = [];
  const hasPlanVariant = item.variants.some(
    (variant) => normalizeConditionValue(variant.conditions.task_profile) === 'chatos_plan',
  );
  const hasDefaultVariant = item.variants.some(
    (variant) => !normalizeConditionValue(variant.conditions.task_profile),
  );

  if (hasDefaultVariant && item.variants.length > 1) {
    entries.push(['mode', '普通模式']);
  }
  if (hasPlanVariant) {
    entries.push(['mode', '规划模式']);
  }

  for (const [label, value] of item.variants.flatMap((variant) => [
    ['project_source_type', variant.conditions.project_source_type || ''] as [string, string],
    ['schedule_mode', variant.conditions.schedule_mode || ''] as [string, string],
  ])) {
    if (value && !entries.some((entry) => entry[0] === label && entry[1] === value)) {
      entries.push([label, value]);
    }
  }
  return entries;
}

function allVariantsUseAllowlist(item: MergedMcpBindingItem): boolean {
  return item.variants.every((variant) => variant.tool_allowlist.length > 0);
}

function toolPolicyPreview(item: MergedMcpBindingItem): string {
  if (item.variants.length === 1) {
    return renderToolPolicyText(
      item.variants[0]?.tool_allowlist || [],
      item.variants[0]?.tool_blocklist || [],
    );
  }
  return item.variants
    .map((variant) => {
      const label = variantDisplayLabel(variant.conditions, item.variants.length);
      return `${label}: ${renderToolPolicyText(variant.tool_allowlist, variant.tool_blocklist)}`;
    })
    .join(' · ');
}

function variantDisplayLabel(conditions: BindingConditions, variantCount: number): string {
  const taskProfile = normalizeConditionValue(conditions.task_profile);
  if (taskProfile === 'chatos_plan') {
    return '规划';
  }
  if (!taskProfile && variantCount > 1) {
    return '普通';
  }
  return '规则';
}

function renderToolPolicyText(allowlist: string[], blocklist: string[]): string {
  const parts = [
    allowlist.length > 0 ? `allow: ${allowlist.slice(0, 3).join(', ')}` : 'allow: *',
    blocklist.length > 0 ? `block: ${blocklist.slice(0, 3).join(', ')}` : null,
  ].filter((value): value is string => Boolean(value));
  return parts.join(' · ');
}

function normalizeConditionValue(value: string | null | undefined): string {
  return (value || '').trim().toLowerCase();
}

function sourceKindLabel(value: string, t: (key: string) => string): string {
  const key = `sourceKind.${value}`;
  const translated = t(key);
  return translated === key ? value : translated;
}
