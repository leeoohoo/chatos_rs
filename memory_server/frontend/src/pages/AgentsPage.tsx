import { useEffect, useMemo, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Input,
  Modal,
  Popconfirm,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { MemoryAgent } from '../types';

const { Text } = Typography;

interface AgentsPageProps {
  filterUserId?: string;
  currentUserId: string;
  isAdmin: boolean;
}

interface AgentEditorState {
  id?: string;
  name: string;
  description: string;
  category: string;
  roleDefinition: string;
  skillIdsText: string;
  enabled: boolean;
}

const EMPTY_EDITOR: AgentEditorState = {
  name: '',
  description: '',
  category: '',
  roleDefinition: '',
  skillIdsText: '',
  enabled: true,
};

export function AgentsPage({ filterUserId, currentUserId, isAdmin }: AgentsPageProps) {
  const { t } = useI18n();
  const [items, setItems] = useState<MemoryAgent[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editor, setEditor] = useState<AgentEditorState>(EMPTY_EDITOR);
  const [aiOpen, setAiOpen] = useState(false);
  const [aiRequirement, setAiRequirement] = useState('');
  const [aiName, setAiName] = useState('');
  const [aiCategory, setAiCategory] = useState('');
  const [aiEnabled, setAiEnabled] = useState(true);

  const scopeUserId = useMemo(() => {
    if (!isAdmin) {
      return currentUserId.trim();
    }
    const filtered = filterUserId?.trim();
    return filtered && filtered.length > 0 ? filtered : currentUserId.trim();
  }, [isAdmin, filterUserId, currentUserId]);

  const isReadonlyForScope = (agent: MemoryAgent): boolean => (
    !isAdmin && agent.user_id !== scopeUserId
  );

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.listAgents(scopeUserId, { limit: 200, offset: 0 });
      setItems(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scopeUserId]);

  const parseSkillIds = (raw: string): string[] => {
    const segments = raw
      .split(/[\n,]/g)
      .map((item) => item.trim())
      .filter((item) => item.length > 0);
    return Array.from(new Set(segments));
  };

  const openCreate = () => {
    setEditor(EMPTY_EDITOR);
    setEditorOpen(true);
  };

  const openEdit = (agent: MemoryAgent) => {
    setEditor({
      id: agent.id,
      name: agent.name || '',
      description: agent.description || '',
      category: agent.category || '',
      roleDefinition: agent.role_definition || '',
      skillIdsText: (agent.skill_ids || []).join(', '),
      enabled: agent.enabled !== false,
    });
    setEditorOpen(true);
  };

  const saveEditor = async () => {
    const name = editor.name.trim();
    const roleDefinition = editor.roleDefinition.trim();
    if (!name || !roleDefinition) {
      setError(t('agents.required'));
      return;
    }

    const skillIds = parseSkillIds(editor.skillIdsText);
    setSaving(true);
    setError(null);
    try {
      if (editor.id) {
        await api.updateAgent(editor.id, {
          name,
          description: editor.description.trim() || undefined,
          category: editor.category.trim() || undefined,
          role_definition: roleDefinition,
          skill_ids: skillIds,
          default_skill_ids: skillIds,
          enabled: editor.enabled,
        });
      } else {
        await api.createAgent({
          user_id: scopeUserId,
          name,
          description: editor.description.trim() || undefined,
          category: editor.category.trim() || undefined,
          role_definition: roleDefinition,
          skill_ids: skillIds,
          default_skill_ids: skillIds,
          enabled: editor.enabled,
        });
      }
      setEditorOpen(false);
      await load();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const removeAgent = async (agentId: string) => {
    setSaving(true);
    setError(null);
    try {
      await api.deleteAgent(agentId);
      await load();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const runAiCreate = async () => {
    const requirement = aiRequirement.trim();
    if (!requirement) {
      setError(t('agents.aiRequired'));
      return;
    }

    setSaving(true);
    setError(null);
    try {
      await api.aiCreateAgent({
        user_id: scopeUserId,
        requirement,
        name: aiName.trim() || undefined,
        category: aiCategory.trim() || undefined,
        enabled: aiEnabled,
      });
      setAiRequirement('');
      setAiName('');
      setAiCategory('');
      setAiEnabled(true);
      setAiOpen(false);
      await load();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const columns: ColumnsType<MemoryAgent> = [
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
      title: t('agents.skills'),
      dataIndex: 'skill_ids',
      key: 'skill_ids',
      width: 220,
      render: (value?: string[]) => (value && value.length > 0 ? value.join(', ') : '-'),
    },
    {
      title: t('agents.status'),
      dataIndex: 'enabled',
      key: 'enabled',
      width: 120,
      render: (value: boolean) => (
        <Tag color={value ? 'green' : 'default'}>{value ? t('common.enabled') : t('common.disabled')}</Tag>
      ),
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
            {readonly && <Tag color="blue">{t('agents.sharedTag')}</Tag>}
            <Button size="small" onClick={() => openEdit(record)} disabled={readonly}>
              {t('common.edit')}
            </Button>
            <Popconfirm
              title={t('agents.deleteConfirm')}
              onConfirm={() => removeAgent(record.id)}
              okButtonProps={{ loading: saving }}
              disabled={readonly}
            >
              <Button size="small" danger disabled={readonly}>
                {t('common.delete')}
              </Button>
            </Popconfirm>
          </Space>
        );
      },
    },
  ];

  return (
    <Card
      title={t('agents.title')}
      extra={
        <Space>
          <Button onClick={load} loading={loading}>
            {t('common.refresh')}
          </Button>
          <Button onClick={() => setAiOpen(true)}>{t('agents.aiCreate')}</Button>
          <Button type="primary" onClick={openCreate}>
            {t('agents.create')}
          </Button>
        </Space>
      }
    >
        {isAdmin && !filterUserId?.trim() && (
          <Alert type="info" showIcon message={t('agents.adminTip')} style={{ marginBottom: 12 }} />
        )}
        {!isAdmin && (
          <Alert
            type="info"
            showIcon
            message={t('agents.sharedReadonlyTip')}
            style={{ marginBottom: 12 }}
          />
        )}
        <Alert
          type="info"
          showIcon
        message={`${t('agents.scopeUser')}: ${scopeUserId || '-'}`}
        style={{ marginBottom: 12 }}
      />
      {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
      <Table
        rowKey="id"
        loading={loading}
        dataSource={items}
        columns={columns}
        pagination={{ pageSize: 20, showSizeChanger: false }}
      />

      <Modal
        open={editorOpen}
        title={editor.id ? t('agents.edit') : t('agents.create')}
        onCancel={() => setEditorOpen(false)}
        onOk={saveEditor}
        confirmLoading={saving}
        width={760}
      >
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          <Input
            value={editor.name}
            onChange={(event) => setEditor((prev) => ({ ...prev, name: event.target.value }))}
            placeholder={t('agents.name')}
          />
          <Input
            value={editor.category}
            onChange={(event) => setEditor((prev) => ({ ...prev, category: event.target.value }))}
            placeholder={t('agents.category')}
          />
          <Input.TextArea
            value={editor.description}
            onChange={(event) => setEditor((prev) => ({ ...prev, description: event.target.value }))}
            placeholder={t('agents.description')}
            rows={3}
          />
          <Input.TextArea
            value={editor.roleDefinition}
            onChange={(event) =>
              setEditor((prev) => ({ ...prev, roleDefinition: event.target.value }))
            }
            placeholder={t('agents.roleDefinition')}
            rows={6}
          />
          <Input.TextArea
            value={editor.skillIdsText}
            onChange={(event) =>
              setEditor((prev) => ({ ...prev, skillIdsText: event.target.value }))
            }
            placeholder={t('agents.skillIds')}
            rows={2}
          />
          <Space>
            <Text>{t('agents.status')}</Text>
            <Switch
              checked={editor.enabled}
              onChange={(checked) => setEditor((prev) => ({ ...prev, enabled: checked }))}
            />
          </Space>
        </Space>
      </Modal>

      <Modal
        open={aiOpen}
        title={t('agents.aiCreate')}
        onCancel={() => setAiOpen(false)}
        onOk={runAiCreate}
        confirmLoading={saving}
      >
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          <Input.TextArea
            value={aiRequirement}
            onChange={(event) => setAiRequirement(event.target.value)}
            placeholder={t('agents.aiRequirement')}
            rows={5}
          />
          <Input
            value={aiName}
            onChange={(event) => setAiName(event.target.value)}
            placeholder={t('agents.nameOptional')}
          />
          <Input
            value={aiCategory}
            onChange={(event) => setAiCategory(event.target.value)}
            placeholder={t('agents.categoryOptional')}
          />
          <Space>
            <Text>{t('agents.status')}</Text>
            <Switch checked={aiEnabled} onChange={setAiEnabled} />
          </Space>
        </Space>
      </Modal>
    </Card>
  );
}
