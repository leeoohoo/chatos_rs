// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { ArrowLeftOutlined, RobotOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Input, Space, Spin, Tabs, Tag, Typography, message } from 'antd';
import { useEffect, useMemo, useState } from 'react';

import { api } from '../../api/client';
import { useI18n } from '../../i18n/I18nProvider';
import { agentDisplayName } from '../../i18n/labels';
import type {
  AgentPromptVendor,
  AgentProviderPromptRecord,
  SystemAgentRecord,
} from '../../types';
import { AgentPromptGenerateModal } from './AgentPromptGenerateModal';
import { AGENT_PROMPT_VENDORS, agentPromptVendorLabel } from './support';

export type PromptVersionSelection =
  | { kind: 'current' }
  | { kind: 'history'; bundleVersion: number };

export function AgentPromptDetailPage({
  agent,
  selection,
  currentBundleVersion,
  onBack,
}: {
  agent: SystemAgentRecord;
  selection: PromptVersionSelection;
  currentBundleVersion: number | null;
  onBack: () => void;
}) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [vendor, setVendor] = useState<AgentPromptVendor>('glm');
  const [drafts, setDrafts] = useState<Partial<Record<AgentPromptVendor, string>>>({});
  const [generateOpen, setGenerateOpen] = useState(false);
  const editable = selection.kind === 'current';
  const promptsQuery = useQuery({
    queryKey: ['agent-provider-prompts', agent.agent_key],
    queryFn: () => api.listAgentProviderPrompts(agent.agent_key),
    enabled: editable,
  });
  const versionQuery = useQuery({
    queryKey: [
      'agent-prompt-version',
      agent.agent_key,
      selection.kind === 'history' ? selection.bundleVersion : null,
    ],
    queryFn: () => api.getAgentPromptVersion(
      agent.agent_key,
      selection.kind === 'history' ? selection.bundleVersion : 0,
    ),
    enabled: selection.kind === 'history',
  });
  const records = useMemo(
    () => new Map((promptsQuery.data || []).map((record) => [record.vendor, record])),
    [promptsQuery.data],
  );
  const snapshots = useMemo(
    () => new Map((versionQuery.data?.prompts || []).map((prompt) => [prompt.vendor, prompt])),
    [versionQuery.data],
  );
  const current = records.get(vendor);
  const snapshot = snapshots.get(vendor);
  const content = editable
    ? drafts[vendor] ?? current?.draft_content ?? current?.published_content ?? ''
    : snapshot?.content || '';
  const revision = editable ? current?.published_revision || 0 : snapshot?.revision || 0;
  const checksum = editable ? current?.published_checksum : snapshot?.checksum;
  const draftChanged = editable && (
    (content || '').trim() !== (current?.published_content || '').trim()
  );

  useEffect(() => {
    setVendor('glm');
    setDrafts({});
    setGenerateOpen(false);
  }, [agent.agent_key, selection]);

  useEffect(() => {
    if (!editable || !promptsQuery.data) {
      return;
    }
    setDrafts((existing) => {
      const next = { ...existing };
      for (const record of promptsQuery.data) {
        if (next[record.vendor] === undefined) {
          next[record.vendor] = record.draft_content || record.published_content || '';
        }
      }
      return next;
    });
  }, [editable, promptsQuery.data]);

  const saveMutation = useMutation({
    mutationFn: async ({ publish }: { publish: boolean }) => {
      const saved = await api.updateAgentProviderPromptDraft(
        agent.agent_key,
        vendor,
        content,
        current?.updated_at,
      );
      return publish
        ? api.publishAgentProviderPrompt(agent.agent_key, vendor)
        : saved;
    },
    onSuccess: (record, variables) => {
      setDrafts((existing) => ({
        ...existing,
        [record.vendor]: record.draft_content || record.published_content || '',
      }));
      queryClient.setQueryData<AgentProviderPromptRecord[]>(
        ['agent-provider-prompts', agent.agent_key],
        (existing = []) => [
          ...existing.filter((item) => item.vendor !== record.vendor),
          record,
        ],
      );
      void queryClient.invalidateQueries({ queryKey: ['agent-prompt-completeness'] });
      if (variables.publish) {
        void queryClient.invalidateQueries({ queryKey: ['agent-prompt-versions', agent.agent_key] });
      }
      message.success(variables.publish ? t('agent.promptPublished') : t('agent.promptDraftSaved'));
    },
    onError: (error) => message.error((error as Error).message),
  });

  const loading = editable ? promptsQuery.isLoading : versionQuery.isLoading;
  const detailVersion = selection.kind === 'history'
    ? selection.bundleVersion
    : currentBundleVersion;

  return (
    <div className="page prompt-detail-page">
      <div className="page-toolbar prompt-page-heading">
        <Space orientation="vertical" size={6}>
          <Button type="link" className="page-back-button" icon={<ArrowLeftOutlined />} onClick={onBack}>
            {t('agent.promptBackToVersions')}
          </Button>
          <Typography.Title level={3}>
            {agentDisplayName(agent, t)} · {t('agent.promptSettings')}
          </Typography.Title>
          <Space wrap>
            <Typography.Text type="secondary">{agent.agent_key}</Typography.Text>
            {detailVersion ? <Tag color="blue">Bundle v{detailVersion}</Tag> : null}
            {!editable ? <Tag>{t('agent.promptVersionReadonly')}</Tag> : null}
          </Space>
        </Space>
      </div>
      <Alert
        className="prompt-page-notice"
        type={editable ? 'info' : 'warning'}
        showIcon
        title={editable ? t('agent.promptNotice') : t('agent.promptHistoryNotice')}
      />
      <div className="prompt-editor-panel">
        <Tabs
          activeKey={vendor}
          onChange={(value) => setVendor(value as AgentPromptVendor)}
          items={AGENT_PROMPT_VENDORS.map((item) => ({
            key: item,
            label: agentPromptVendorLabel(item),
          }))}
        />
        {loading ? (
          <div className="prompt-editor-loading">
            <Spin />
            <Typography.Text type="secondary">{t('agent.promptLoading')}</Typography.Text>
          </div>
        ) : (
          <Space orientation="vertical" size="middle" className="prompt-editor-content">
            <Space wrap>
              <Tag color={revision ? 'green' : 'orange'}>Revision {revision}</Tag>
              {editable ? (
                <Tag color={draftChanged ? 'orange' : 'blue'}>
                  {draftChanged
                    ? t('agent.promptVersionDraftChanged')
                    : t('agent.promptDraftSynced')}
                </Tag>
              ) : null}
              <Typography.Text type="secondary" copyable={Boolean(checksum)}>
                {checksum || t('agent.promptNotPublished')}
              </Typography.Text>
            </Space>
            <Input.TextArea
              className="prompt-content-editor"
              value={content}
              rows={24}
              maxLength={64 * 1024}
              showCount
              readOnly={!editable}
              onChange={(event) => setDrafts((existing) => ({
                ...existing,
                [vendor]: event.target.value,
              }))}
            />
            {editable ? (
              <Space wrap>
                <Button icon={<RobotOutlined />} onClick={() => setGenerateOpen(true)}>
                  {t('agent.promptGenerate')}
                </Button>
                <Button
                  loading={saveMutation.isPending}
                  onClick={() => saveMutation.mutate({ publish: false })}
                >
                  {t('agent.promptSaveDraft')}
                </Button>
                <Button
                  type="primary"
                  loading={saveMutation.isPending}
                  onClick={() => saveMutation.mutate({ publish: true })}
                >
                  {t('agent.promptPublish')}
                </Button>
              </Space>
            ) : null}
          </Space>
        )}
      </div>
      {editable && generateOpen ? (
        <AgentPromptGenerateModal
          open={generateOpen}
          agentKey={agent.agent_key}
          vendor={vendor}
          currentContent={content}
          onClose={() => setGenerateOpen(false)}
          onGenerated={(generated) => setDrafts((existing) => ({
            ...existing,
            [vendor]: generated,
          }))}
        />
      ) : null}
    </div>
  );
}
