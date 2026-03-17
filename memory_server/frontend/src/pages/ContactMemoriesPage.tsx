import { useEffect, useMemo, useState } from 'react';
import { Alert, Button, Card, Empty, Select, Space, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { AgentRecall, MemoryContact, ProjectMemory } from '../types';

const { Text, Paragraph } = Typography;

interface ContactMemoriesPageProps {
  filterUserId?: string;
  currentUserId: string;
  isAdmin: boolean;
  mode: 'project' | 'recall';
}

function formatDateTime(value?: string | null): string {
  if (!value) {
    return '-';
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return parsed.toLocaleString();
}

function sortByUpdatedDesc<T extends { updated_at?: string }>(items: T[]): T[] {
  return [...items].sort((a, b) => {
    const lhs = new Date(a.updated_at || 0).getTime();
    const rhs = new Date(b.updated_at || 0).getTime();
    return rhs - lhs;
  });
}

export function ContactMemoriesPage({
  filterUserId,
  currentUserId,
  isAdmin,
  mode,
}: ContactMemoriesPageProps) {
  const { t } = useI18n();
  const [contacts, setContacts] = useState<MemoryContact[]>([]);
  const [selectedContactId, setSelectedContactId] = useState<string | undefined>(undefined);
  const [selectedProjectId, setSelectedProjectId] = useState<string | undefined>(undefined);
  const [projectMemories, setProjectMemories] = useState<ProjectMemory[]>([]);
  const [agentRecalls, setAgentRecalls] = useState<AgentRecall[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const scopeUserId = useMemo(() => {
    if (!isAdmin) {
      return currentUserId.trim();
    }
    const filtered = filterUserId?.trim();
    return filtered && filtered.length > 0 ? filtered : currentUserId.trim();
  }, [isAdmin, filterUserId, currentUserId]);

  const contactOptions = useMemo(
    () =>
      contacts.map((item) => ({
        value: item.id,
        label: `${item.agent_name_snapshot || item.agent_id} (${item.agent_id})`,
      })),
    [contacts],
  );

  const projectRows = useMemo(() => {
    const latestByProject = new Map<string, ProjectMemory>();
    for (const item of projectMemories) {
      if (!item.project_id) {
        continue;
      }
      const existing = latestByProject.get(item.project_id);
      if (!existing) {
        latestByProject.set(item.project_id, item);
        continue;
      }
      const existingTs = new Date(existing.updated_at || 0).getTime();
      const currentTs = new Date(item.updated_at || 0).getTime();
      if (currentTs >= existingTs) {
        latestByProject.set(item.project_id, item);
      }
    }
    return sortByUpdatedDesc(Array.from(latestByProject.values()));
  }, [projectMemories]);

  const selectedProjectMemory = useMemo(() => {
    if (!selectedProjectId) {
      return null;
    }
    const candidates = projectMemories.filter((item) => item.project_id === selectedProjectId);
    if (candidates.length === 0) {
      return null;
    }
    return sortByUpdatedDesc(candidates)[0];
  }, [projectMemories, selectedProjectId]);

  const loadContacts = async () => {
    const rows = await api.listContacts(scopeUserId, { status: 'active', limit: 500, offset: 0 });
    const normalized = rows.filter((item) => item.status === 'active' || !item.status);
    setContacts(normalized);
    if (normalized.length === 0) {
      setSelectedContactId(undefined);
      return;
    }
    if (selectedContactId && normalized.some((item) => item.id === selectedContactId)) {
      return;
    }
    setSelectedContactId(normalized[0].id);
  };

  const loadProjectMemories = async (contactId: string) => {
    const rows = await api.listContactProjectMemories(contactId, { limit: 1000, offset: 0 });
    const sorted = sortByUpdatedDesc(rows);
    setProjectMemories(sorted);
    if (sorted.length === 0) {
      setSelectedProjectId(undefined);
      return;
    }
    if (selectedProjectId && sorted.some((item) => item.project_id === selectedProjectId)) {
      return;
    }
    setSelectedProjectId(sorted[0].project_id);
  };

  const loadAgentRecalls = async (contactId: string) => {
    const rows = await api.listContactAgentRecalls(contactId, { limit: 1000, offset: 0 });
    setAgentRecalls(sortByUpdatedDesc(rows));
  };

  const loadAll = async () => {
    setLoading(true);
    setError(null);
    try {
      await loadContacts();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadAll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scopeUserId]);

  useEffect(() => {
    if (!selectedContactId) {
      setProjectMemories([]);
      setAgentRecalls([]);
      setSelectedProjectId(undefined);
      return;
    }

    const run = async () => {
      setLoading(true);
      setError(null);
      try {
        if (mode === 'project') {
          await loadProjectMemories(selectedContactId);
        } else {
          await loadAgentRecalls(selectedContactId);
        }
      } catch (err) {
        setError((err as Error).message);
      } finally {
        setLoading(false);
      }
    };

    void run();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, selectedContactId]);

  const projectColumns: ColumnsType<ProjectMemory> = [
    {
      title: t('memory.projectId'),
      dataIndex: 'project_id',
      key: 'project_id',
      render: (value: string) => <Text code>{value || '-'}</Text>,
    },
    {
      title: t('memory.memoryVersion'),
      dataIndex: 'memory_version',
      key: 'memory_version',
      width: 140,
    },
    {
      title: t('memory.recallSummarized'),
      dataIndex: 'recall_summarized',
      key: 'recall_summarized',
      width: 150,
      render: (value?: number) => (Number(value) === 1 ? 'Yes' : 'No'),
    },
    {
      title: t('memory.lastSourceAt'),
      dataIndex: 'last_source_at',
      key: 'last_source_at',
      width: 220,
      render: (value?: string | null) => formatDateTime(value),
    },
    {
      title: t('memory.updatedAt'),
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 220,
      render: (value: string) => formatDateTime(value),
    },
  ];

  const recallColumns: ColumnsType<AgentRecall> = [
    {
      title: t('memory.recallKey'),
      dataIndex: 'recall_key',
      key: 'recall_key',
      width: 220,
      render: (value: string) => <Text code>{value || '-'}</Text>,
    },
    {
      title: t('memory.recallText'),
      dataIndex: 'recall_text',
      key: 'recall_text',
      render: (value: string) => (
        <Paragraph style={{ marginBottom: 0, whiteSpace: 'pre-wrap' }}>
          {value || '-'}
        </Paragraph>
      ),
    },
    {
      title: t('memory.recallLevel'),
      dataIndex: 'level',
      key: 'level',
      width: 100,
      render: (value?: number) => (
        Number.isFinite(Number(value)) ? Number(value) : 0
      ),
    },
    {
      title: t('memory.sourceProjects'),
      dataIndex: 'source_project_ids',
      key: 'source_project_ids',
      width: 220,
      render: (value: string[]) => (
        value && value.length > 0 ? value.join(', ') : '-'
      ),
    },
    {
      title: t('memory.confidence'),
      dataIndex: 'confidence',
      key: 'confidence',
      width: 120,
      render: (value?: number | null) => (
        typeof value === 'number' ? value.toFixed(2) : '-'
      ),
    },
    {
      title: t('memory.lastSeenAt'),
      dataIndex: 'last_seen_at',
      key: 'last_seen_at',
      width: 220,
      render: (value?: string | null) => formatDateTime(value),
    },
    {
      title: t('memory.updatedAt'),
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 220,
      render: (value: string) => formatDateTime(value),
    },
  ];

  return (
    <Space direction="vertical" size={12} style={{ width: '100%' }}>
      <Card
        title={mode === 'project' ? t('memory.projectSummaryTitle') : t('memory.agentRecallTitle')}
        extra={(
          <Button onClick={loadAll} loading={loading}>
            {t('common.refresh')}
          </Button>
        )}
      >
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          <Alert
            type="info"
            showIcon
            message={`${t('memory.scopeUser')}: ${scopeUserId || '-'}`}
          />
          {error && <Alert type="error" showIcon message={error} />}
          <Space align="center" wrap>
            <Text strong>{t('memory.contact')}:</Text>
            <Select
              style={{ width: 520 }}
              placeholder={t('memory.contactPlaceholder')}
              loading={loading}
              value={selectedContactId}
              options={contactOptions}
              onChange={(value) => setSelectedContactId(value)}
              allowClear
            />
          </Space>
          {contacts.length === 0 && (
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('memory.noContacts')} />
          )}
        </Space>
      </Card>

      {mode === 'project' ? (
        <Card title={t('memory.projectListTitle')}>
          <Space direction="vertical" size={12} style={{ width: '100%' }}>
            <Table
              rowKey={(record) => record.project_id || record.id}
              loading={loading}
              dataSource={projectRows}
              columns={projectColumns}
              pagination={{ pageSize: 10, showSizeChanger: false }}
              locale={{ emptyText: t('memory.emptyProject') }}
              onRow={(record) => ({
                onClick: () => setSelectedProjectId(record.project_id),
              })}
            />

            {selectedProjectMemory ? (
              <Card
                size="small"
                title={(
                  <Space>
                    <Text>{t('memory.projectId')}</Text>
                    <Tag color="blue">{selectedProjectMemory.project_id}</Tag>
                  </Space>
                )}
              >
                <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                  {selectedProjectMemory.memory_text || '-'}
                </Paragraph>
              </Card>
            ) : (
              <Alert type="info" showIcon message={t('memory.selectProjectHint')} />
            )}
          </Space>
        </Card>
      ) : (
        <Card title={t('memory.agentRecallTitle')}>
          <Table
            rowKey="id"
            loading={loading}
            dataSource={agentRecalls}
            columns={recallColumns}
            pagination={{ pageSize: 10, showSizeChanger: false }}
            locale={{ emptyText: t('memory.emptyRecall') }}
          />
        </Card>
      )}
    </Space>
  );
}
