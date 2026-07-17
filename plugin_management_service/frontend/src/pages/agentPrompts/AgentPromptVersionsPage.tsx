// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  ArrowLeftOutlined,
  EditOutlined,
  EyeOutlined,
  HistoryOutlined,
} from '@ant-design/icons';
import { useQuery } from '@tanstack/react-query';
import { Alert, Button, Empty, Space, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useMemo, useState } from 'react';

import { api } from '../../api/client';
import { useI18n } from '../../i18n/I18nProvider';
import { agentDisplayName } from '../../i18n/labels';
import type {
  AgentPromptVersionSummary,
  CurrentUser,
} from '../../types';
import { AgentPromptDetailPage, type PromptVersionSelection } from './AgentPromptDetailPage';
import { AGENT_PROMPT_VENDORS, agentPromptVendorLabel } from './support';

export function AgentPromptVersionsPage({
  user,
  agentKey,
  onBack,
}: {
  user: CurrentUser;
  agentKey: string;
  onBack: () => void;
}) {
  const { locale, t } = useI18n();
  const [selection, setSelection] = useState<PromptVersionSelection | null>(null);
  const isAdmin = user.role === 'super_admin';
  const agentsQuery = useQuery({
    queryKey: ['system-agents'],
    queryFn: api.listSystemAgents,
    enabled: isAdmin,
  });
  const promptsQuery = useQuery({
    queryKey: ['agent-provider-prompts', agentKey],
    queryFn: () => api.listAgentProviderPrompts(agentKey),
    enabled: isAdmin,
  });
  const versionsQuery = useQuery({
    queryKey: ['agent-prompt-versions', agentKey],
    queryFn: () => api.listAgentPromptVersions(agentKey),
    enabled: isAdmin,
  });
  const agent = agentsQuery.data?.find((item) => item.agent_key === agentKey) || null;
  const versions = versionsQuery.data || [];
  const latestVersion = versions[0] || null;
  const hasDraftChanges = useMemo(
    () => (promptsQuery.data || []).some((record) => (
      (record.draft_content || '').trim() !== (record.published_content || '').trim()
    )),
    [promptsQuery.data],
  );

  const columns = useMemo<ColumnsType<AgentPromptVersionSummary>>(
    () => [
      {
        title: t('agent.promptVersion'),
        dataIndex: 'bundle_version',
        width: 190,
        render: (bundleVersion: number, record) => (
          <Space orientation="vertical" size={2}>
            <Space size={8}>
              <Typography.Text strong>Bundle v{bundleVersion}</Typography.Text>
              {record.id === latestVersion?.id ? (
                <Tag color="blue">{t('agent.promptVersionCurrent')}</Tag>
              ) : null}
            </Space>
            <Typography.Text type="secondary" className="prompt-version-id">
              {record.id}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: t('agent.promptVersionRevisions'),
        key: 'vendor_revisions',
        render: (_, record) => {
          const revisions = new Map(
            record.vendor_revisions.map((item) => [item.vendor, item.revision]),
          );
          return (
            <Space wrap size={[6, 6]}>
              {AGENT_PROMPT_VENDORS.map((vendor) => (
                <Tag key={vendor} color={revisions.has(vendor) ? 'default' : 'warning'}>
                  {agentPromptVendorLabel(vendor)} · r{revisions.get(vendor) || 0}
                </Tag>
              ))}
            </Space>
          );
        },
      },
      {
        title: t('table.status'),
        key: 'status',
        width: 160,
        render: (_, record) => {
          const isLatest = record.id === latestVersion?.id;
          if (isLatest && hasDraftChanges) {
            return <Tag color="orange">{t('agent.promptVersionDraftChanged')}</Tag>;
          }
          return (
            <Space orientation="vertical" size={2}>
              <Tag color={isLatest ? 'green' : 'default'}>
                {isLatest
                  ? t('agent.promptVersionPublishedCurrent')
                  : t('agent.promptVersionPublished')}
              </Tag>
              {record.changed_vendor ? (
                <Typography.Text type="secondary" className="prompt-version-change">
                  {t('agent.promptVersionChangedVendor', {
                    vendor: agentPromptVendorLabel(record.changed_vendor),
                  })}
                </Typography.Text>
              ) : null}
            </Space>
          );
        },
      },
      {
        title: t('agent.promptVersionPublisher'),
        key: 'published',
        width: 210,
        render: (_, record) => (
          <Space orientation="vertical" size={2}>
            <Typography.Text>{formatTimestamp(record.published_at, locale)}</Typography.Text>
            <Typography.Text type="secondary">{record.published_by}</Typography.Text>
          </Space>
        ),
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 112,
        render: (_, record) => {
          const isLatest = record.id === latestVersion?.id;
          return (
            <Button
              type={isLatest ? 'primary' : 'default'}
              icon={isLatest ? <EditOutlined /> : <EyeOutlined />}
              onClick={() => setSelection(
                isLatest
                  ? { kind: 'current' }
                  : { kind: 'history', bundleVersion: record.bundle_version },
              )}
            >
              {isLatest ? t('common.edit') : t('common.view')}
            </Button>
          );
        },
      },
    ],
    [hasDraftChanges, latestVersion?.id, locale, t],
  );

  if (!isAdmin) {
    return <Alert type="error" showIcon message={t('admin.only')} />;
  }

  if (selection && agent) {
    return (
      <AgentPromptDetailPage
        agent={agent}
        selection={selection}
        currentBundleVersion={latestVersion?.bundle_version || null}
        onBack={() => setSelection(null)}
      />
    );
  }

  const displayName = agent ? agentDisplayName(agent, t) : agentKey;

  return (
    <div className="page prompt-version-page">
      <div className="page-toolbar prompt-page-heading">
        <Space orientation="vertical" size={6}>
          <Button type="link" className="page-back-button" icon={<ArrowLeftOutlined />} onClick={onBack}>
            {t('agent.promptBackToAgents')}
          </Button>
          <Space align="center" size={10}>
            <HistoryOutlined className="prompt-page-icon" />
            <Typography.Title level={3}>
              {displayName} · {t('agent.promptVersionsTitle')}
            </Typography.Title>
          </Space>
          <Typography.Text type="secondary">{agentKey}</Typography.Text>
        </Space>
      </div>
      <Alert className="prompt-page-notice" type="info" showIcon title={t('agent.promptVersionsNotice')} />
      <Table
        rowKey="id"
        className="prompt-version-table"
        columns={columns}
        dataSource={versions}
        loading={versionsQuery.isLoading || promptsQuery.isLoading || agentsQuery.isLoading}
        pagination={false}
        tableLayout="fixed"
        scroll={{ x: 980 }}
        locale={{
          emptyText: (
            <Empty description={t('agent.promptVersionEmpty')}>
              <Button type="primary" onClick={() => setSelection({ kind: 'current' })}>
                {t('agent.promptOpenEditor')}
              </Button>
            </Empty>
          ),
        }}
        onRow={(record) => ({
          onDoubleClick: () => setSelection(
            record.id === latestVersion?.id
              ? { kind: 'current' }
              : { kind: 'history', bundleVersion: record.bundle_version },
          ),
        })}
      />
    </div>
  );
}

function formatTimestamp(value: string, locale: string): string {
  const timestamp = new Date(value);
  if (Number.isNaN(timestamp.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat(locale, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(timestamp);
}
