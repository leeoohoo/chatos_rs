// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Input, Modal, Space, Tabs, Tag, Typography, message } from 'antd';
import { RobotOutlined } from '@ant-design/icons';
import { useEffect, useMemo, useState } from 'react';

import { api } from '../../api/client';
import { useI18n } from '../../i18n/I18nProvider';
import type {
  AgentPromptVendor,
  AgentProviderPromptRecord,
  SystemAgentRecord,
} from '../../types';
import { AgentPromptGenerateModal } from './AgentPromptGenerateModal';

const VENDORS: AgentPromptVendor[] = ['glm', 'deepseek', 'gpt', 'kimi'];

export function AgentPromptModal({
  agent,
  open,
  onClose,
}: {
  agent: SystemAgentRecord | null;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [vendor, setVendor] = useState<AgentPromptVendor>('glm');
  const [drafts, setDrafts] = useState<Partial<Record<AgentPromptVendor, string>>>({});
  const [generateOpen, setGenerateOpen] = useState(false);

  const promptsQuery = useQuery({
    queryKey: ['agent-provider-prompts', agent?.agent_key],
    queryFn: () => api.listAgentProviderPrompts(agent?.agent_key || ''),
    enabled: open && Boolean(agent?.agent_key),
  });

  const records = useMemo(
    () => new Map((promptsQuery.data || []).map((record) => [record.vendor, record])),
    [promptsQuery.data],
  );
  const current = records.get(vendor);
  const content = drafts[vendor] ?? current?.draft_content ?? current?.published_content ?? '';

  useEffect(() => {
    if (!open) {
      setVendor('glm');
      setDrafts({});
      return;
    }
    if (!promptsQuery.data) return;
    setDrafts((existing) => {
      const next = { ...existing };
      for (const record of promptsQuery.data) {
        if (next[record.vendor] === undefined) {
          next[record.vendor] = record.draft_content || record.published_content || '';
        }
      }
      return next;
    });
  }, [open, promptsQuery.data]);

  const saveMutation = useMutation({
    mutationFn: async ({ publish }: { publish: boolean }) => {
      if (!agent) throw new Error(t('agent.promptMissingAgent'));
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
      queryClient.setQueryData<AgentProviderPromptRecord[]>(
        ['agent-provider-prompts', agent?.agent_key],
        (existing = []) => [
          ...existing.filter((item) => item.vendor !== record.vendor),
          record,
        ],
      );
      void queryClient.invalidateQueries({ queryKey: ['agent-prompt-completeness'] });
      message.success(variables.publish ? t('agent.promptPublished') : t('agent.promptDraftSaved'));
    },
    onError: (error) => message.error((error as Error).message),
  });

  return (
    <Modal
      title={agent ? `${agent.display_name} · ${t('agent.promptSettings')}` : t('agent.promptSettings')}
      open={open}
      width={980}
      footer={null}
      onCancel={onClose}
      destroyOnClose
    >
      <Alert type="info" showIcon message={t('agent.promptNotice')} />
      <Tabs
        activeKey={vendor}
        onChange={(value) => setVendor(value as AgentPromptVendor)}
        items={VENDORS.map((item) => ({
          key: item,
          label: vendorLabel(item),
        }))}
      />
      {promptsQuery.isLoading ? (
        <Typography.Text type="secondary">{t('agent.promptLoading')}</Typography.Text>
      ) : (
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <Space wrap>
            <Tag color={current?.published_revision ? 'green' : 'orange'}>
              Revision {current?.published_revision || 0}
            </Tag>
            <Typography.Text type="secondary" copyable={Boolean(current?.published_checksum)}>
              {current?.published_checksum || t('agent.promptNotPublished')}
            </Typography.Text>
          </Space>
          <Input.TextArea
            value={content}
            rows={22}
            maxLength={64 * 1024}
            showCount
            onChange={(event) => setDrafts((existing) => ({
              ...existing,
              [vendor]: event.target.value,
            }))}
          />
          <Space>
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
          <AgentPromptGenerateModal
            open={generateOpen}
            agentKey={agent?.agent_key || ''}
            vendor={vendor}
            currentContent={content}
            onClose={() => setGenerateOpen(false)}
            onGenerated={(generated) => setDrafts((existing) => ({
              ...existing,
              [vendor]: generated,
            }))}
          />
        </Space>
      )}
    </Modal>
  );
}

function vendorLabel(vendor: AgentPromptVendor): string {
  return {
    glm: 'GLM',
    deepseek: 'DeepSeek',
    gpt: 'GPT / OpenAI',
    kimi: 'Kimi / Moonshot',
  }[vendor];
}
