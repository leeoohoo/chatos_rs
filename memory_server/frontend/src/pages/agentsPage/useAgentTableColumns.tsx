import { Button, Popconfirm, Space, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';

import type { MemoryAgent } from '../../types';
import type { AgentPageTranslate } from './types';

const { Text } = Typography;

interface UseAgentTableColumnsParams {
  t: AgentPageTranslate;
  saving: boolean;
  isReadonlyForScope: (agent: MemoryAgent) => boolean;
  resolvePluginDisplayName: (pluginSource: string) => string;
  resolveSkillDisplayName: (agent: MemoryAgent, skillId: string) => string;
  resolveModelDisplayName: (modelConfigId?: string | null) => string;
  onOpenConversation: (agent: MemoryAgent) => void | Promise<void>;
  onOpenEdit: (agent: MemoryAgent) => void;
  onOpenPluginPreview: (pluginSource: string) => void | Promise<void>;
  onOpenSkillPreview: (agent: MemoryAgent, skillId: string) => void | Promise<void>;
  onRemoveAgent: (agentId: string) => void | Promise<void>;
}

export function useAgentTableColumns({
  t,
  saving,
  isReadonlyForScope,
  resolvePluginDisplayName,
  resolveSkillDisplayName,
  resolveModelDisplayName,
  onOpenConversation,
  onOpenEdit,
  onOpenPluginPreview,
  onOpenSkillPreview,
  onRemoveAgent,
}: UseAgentTableColumnsParams): ColumnsType<MemoryAgent> {
  return [
    {
      title: t('agents.name'),
      dataIndex: 'name',
      key: 'name',
      render: (value: string, record) => (
        <Space direction="vertical" size={0}>
          <Text strong>{value || record.id.slice(0, 8)}</Text>
          <Text type="secondary" style={{ fontSize: 12 }}>
            {record.id.slice(0, 8)}
          </Text>
          <Button
            type="link"
            size="small"
            style={{ paddingInline: 0, height: 20 }}
            onClick={(event) => {
              event.stopPropagation();
              void onOpenConversation(record);
            }}
          >
            {t('agents.viewChats')}
          </Button>
        </Space>
      ),
    },
    {
      title: t('agents.category'),
      dataIndex: 'category',
      key: 'category',
      width: 140,
      render: (value?: string | null) => <Tag>{value || '-'}</Tag>,
    },
    {
      title: t('agents.plugins'),
      dataIndex: 'plugin_sources',
      key: 'plugin_sources',
      width: 220,
      render: (value: string[] | undefined) => {
        if (!value || value.length === 0) {
          return '-';
        }
        return (
          <Space size={[4, 4]} wrap>
            {value.map((pluginSource) => (
              <Button
                key={pluginSource}
                type="link"
                size="small"
                style={{ paddingInline: 0, height: 20 }}
                onClick={(event) => {
                  event.stopPropagation();
                  void onOpenPluginPreview(pluginSource);
                }}
              >
                {resolvePluginDisplayName(pluginSource)}
              </Button>
            ))}
          </Space>
        );
      },
    },
    {
      title: t('agents.skills'),
      dataIndex: 'skill_ids',
      key: 'skill_ids',
      width: 220,
      render: (value: string[] | undefined, record) => {
        if (!value || value.length === 0) {
          return '-';
        }
        return (
          <Space size={[4, 4]} wrap>
            {value.map((skillId) => (
              <Button
                key={skillId}
                type="link"
                size="small"
                style={{ paddingInline: 0, height: 20 }}
                onClick={(event) => {
                  event.stopPropagation();
                  void onOpenSkillPreview(record, skillId);
                }}
              >
                {resolveSkillDisplayName(record, skillId)}
              </Button>
            ))}
          </Space>
        );
      },
    },
    {
      title: t('agents.status'),
      dataIndex: 'enabled',
      key: 'enabled',
      width: 120,
      render: (value: boolean) => (
        <Tag color={value ? 'green' : 'default'}>
          {value ? t('common.enabled') : t('common.disabled')}
        </Tag>
      ),
    },
    {
      title: t('agents.executionModel'),
      dataIndex: 'model_config_id',
      key: 'model_config_id',
      width: 220,
      render: (value?: string | null) => resolveModelDisplayName(value),
    },
    {
      title: t('agents.updatedAt'),
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 220,
      render: (value: string) => new Date(value).toLocaleString(),
    },
    {
      title: t('common.action'),
      key: 'action',
      width: 180,
      render: (_, record) => {
        const readonly = isReadonlyForScope(record);
        return (
          <Space>
            <Button
              size="small"
              onClick={(event) => {
                event.stopPropagation();
                void onOpenConversation(record);
              }}
            >
              {t('agents.viewChats')}
            </Button>
            {readonly && <Tag color="blue">{t('agents.sharedTag')}</Tag>}
            <Button
              size="small"
              onClick={(event) => {
                event.stopPropagation();
                onOpenEdit(record);
              }}
              disabled={readonly}
            >
              {t('common.edit')}
            </Button>
            <Popconfirm
              title={t('agents.deleteConfirm')}
              onConfirm={() => onRemoveAgent(record.id)}
              okButtonProps={{ loading: saving }}
              disabled={readonly}
            >
              <Button
                size="small"
                danger
                disabled={readonly}
                onClick={(event) => event.stopPropagation()}
              >
                {t('common.delete')}
              </Button>
            </Popconfirm>
          </Space>
        );
      },
    },
  ];
}
