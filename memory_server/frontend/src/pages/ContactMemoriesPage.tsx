import { useEffect, useMemo, useState } from 'react';
import { Alert, Button, Card, Empty, Grid, List, Space, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { AgentRecall, ContactProject, MemoryContact, SessionSummary } from '../types';

const { Text, Paragraph } = Typography;
const { useBreakpoint } = Grid;

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

function sortByCreatedDesc<T extends { created_at?: string }>(items: T[]): T[] {
  return [...items].sort((a, b) => {
    const lhs = new Date(a.created_at || 0).getTime();
    const rhs = new Date(b.created_at || 0).getTime();
    return rhs - lhs;
  });
}

function normalizeProjectId(projectId?: string | null): string {
  const raw = typeof projectId === 'string' ? projectId.trim() : '';
  return raw || '0';
}

export function ContactMemoriesPage({
  filterUserId,
  currentUserId,
  isAdmin,
  mode,
}: ContactMemoriesPageProps) {
  const { t } = useI18n();
  const screens = useBreakpoint();
  const [contacts, setContacts] = useState<MemoryContact[]>([]);
  const [selectedContactId, setSelectedContactId] = useState<string | undefined>(undefined);
  const [selectedProjectId, setSelectedProjectId] = useState<string | undefined>(undefined);
  const [contactProjects, setContactProjects] = useState<ContactProject[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | undefined>(undefined);
  const [sessionSummaries, setSessionSummaries] = useState<SessionSummary[]>([]);
  const [agentRecalls, setAgentRecalls] = useState<AgentRecall[]>([]);
  const [selectedRecallId, setSelectedRecallId] = useState<string | undefined>(undefined);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const scopeUserId = useMemo(() => {
    if (!isAdmin) {
      return currentUserId.trim();
    }
    const filtered = filterUserId?.trim();
    return filtered && filtered.length > 0 ? filtered : currentUserId.trim();
  }, [isAdmin, filterUserId, currentUserId]);

  const projectRows = useMemo(() => {
    return sortByUpdatedDesc(contactProjects);
  }, [contactProjects]);

  const summaryRows = useMemo(() => {
    return sortByCreatedDesc(sessionSummaries);
  }, [sessionSummaries]);

  const recallRows = useMemo(() => {
    return sortByUpdatedDesc(agentRecalls);
  }, [agentRecalls]);

  const getProjectDisplayName = (project: Pick<ContactProject, 'project_id' | 'project_name'>): string => {
    const named = typeof project.project_name === 'string' ? project.project_name.trim() : '';
    if (named) {
      return named;
    }
    return normalizeProjectId(project.project_id) === '0'
      ? t('memory.unassignedProject')
      : t('memory.unnamedProject');
  };

  const selectedProject = useMemo(() => {
    if (!selectedProjectId) {
      return null;
    }
    return projectRows.find((item) => item.project_id === selectedProjectId) ?? null;
  }, [projectRows, selectedProjectId]);

  const selectedContact = useMemo(() => {
    if (!selectedContactId) {
      return null;
    }
    return contacts.find((item) => item.id === selectedContactId) ?? null;
  }, [contacts, selectedContactId]);

  const selectedRecall = useMemo(() => {
    if (!selectedRecallId) {
      return null;
    }
    return recallRows.find((item) => item.id === selectedRecallId) ?? null;
  }, [recallRows, selectedRecallId]);

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
    const rows = await api.listContactProjects(contactId, { limit: 1000, offset: 0 });
    const sorted = sortByUpdatedDesc(rows);
    setContactProjects(sorted);
    if (sorted.length === 0) {
      setSelectedProjectId(undefined);
      setSelectedSessionId(undefined);
      setSessionSummaries([]);
      return;
    }
    const nextProjectId = selectedProjectId && sorted.some((item) => item.project_id === selectedProjectId)
      ? selectedProjectId
      : sorted[0].project_id;
    setSelectedProjectId(nextProjectId);
    if (!nextProjectId) {
      setSelectedSessionId(undefined);
      setSessionSummaries([]);
    }
  };

  const loadProjectSummaryDetail = async (contactId: string, projectId: string) => {
    const pid = projectId.trim();
    if (!pid) {
      setSelectedSessionId(undefined);
      setSessionSummaries([]);
      return;
    }
    const data = await api.listContactProjectSummaries(contactId, pid);
    setSelectedSessionId(data.session_id ?? undefined);
    setSessionSummaries(sortByCreatedDesc(data.items));
  };

  const loadAgentRecalls = async (contactId: string) => {
    const rows = await api.listContactAgentRecalls(contactId, { limit: 1000, offset: 0 });
    const sorted = sortByUpdatedDesc(rows);
    setAgentRecalls(sorted);
    if (sorted.length === 0) {
      setSelectedRecallId(undefined);
      return;
    }
    const nextRecallId = selectedRecallId && sorted.some((item) => item.id === selectedRecallId)
      ? selectedRecallId
      : sorted[0].id;
    setSelectedRecallId(nextRecallId);
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
      setContactProjects([]);
      setSelectedSessionId(undefined);
      setSessionSummaries([]);
      setAgentRecalls([]);
      setSelectedRecallId(undefined);
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

  useEffect(() => {
    if (mode !== 'project' || !selectedContactId || !selectedProjectId) {
      return;
    }
    const run = async () => {
      try {
        await loadProjectSummaryDetail(selectedContactId, selectedProjectId);
      } catch (err) {
        setError((err as Error).message);
      }
    };
    void run();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, selectedContactId, selectedProjectId]);

  const summaryColumns: ColumnsType<SessionSummary> = [
    {
      title: t('summaryLevels.level'),
      dataIndex: 'level',
      key: 'level',
      width: 90,
      render: (value?: number) => <Tag color="blue">L{Number(value) || 0}</Tag>,
    },
    {
      title: t('summaryLevels.status'),
      dataIndex: 'status',
      key: 'status',
      width: 140,
      render: (value?: string, record?: SessionSummary) => {
        const summarized = value === 'summarized' || Boolean(record?.rollup_summary_id);
        return (
          <Tag color={summarized ? 'default' : 'processing'}>
            {summarized ? 'summarized' : (value || 'pending')}
          </Tag>
        );
      },
    },
    {
      title: t('memory.agentMemoryStatus'),
      dataIndex: 'agent_memory_summarized',
      key: 'agent_memory_summarized',
      width: 140,
      render: (value?: number) => (
        <Tag color={Number(value) === 1 ? 'green' : 'default'}>
          {Number(value) === 1 ? t('memory.agentMemoryDone') : t('memory.agentMemoryPending')}
        </Tag>
      ),
    },
    {
      title: t('memory.recallKey'),
      dataIndex: 'trigger_type',
      key: 'trigger_type',
      render: (value?: string) => <Text code>{value || '-'}</Text>,
    },
    {
      title: t('sessionDetail.sourceCount'),
      dataIndex: 'source_message_count',
      key: 'source_message_count',
      width: 120,
      render: (value?: number) => Number(value) || 0,
    },
    {
      title: t('memory.rollupTarget'),
      dataIndex: 'rollup_summary_id',
      key: 'rollup_summary_id',
      width: 160,
      render: (value?: string | null) => (
        value ? <Tag color="purple">linked</Tag> : <Tag bordered={false}>-</Tag>
      ),
    },
    {
      title: t('memory.updatedAt'),
      dataIndex: 'created_at',
      key: 'created_at',
      width: 220,
      render: (value?: string) => formatDateTime(value),
    },
  ];

  const getContactDisplayName = (contact: MemoryContact): string => {
    const named = typeof contact.agent_name_snapshot === 'string' ? contact.agent_name_snapshot.trim() : '';
    return named || contact.agent_id;
  };

  const listPaneHeight = screens.lg ? 620 : undefined;
  const gridTemplateColumns = screens.xl
    ? '280px 320px minmax(0, 1fr)'
    : screens.lg
      ? '240px 280px minmax(0, 1fr)'
      : '1fr';

  const getSelectableItemStyle = (selected: boolean) => ({
    cursor: 'pointer',
    padding: 14,
    borderRadius: 12,
    border: `1px solid ${selected ? '#91caff' : '#f0f0f0'}`,
    background: selected ? '#e6f4ff' : '#fff',
    boxShadow: selected ? '0 0 0 1px rgba(22, 119, 255, 0.06)' : 'none',
    transition: 'all 0.2s ease',
  });

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
          {contacts.length === 0 && (
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('memory.noContacts')} />
          )}
        </Space>
      </Card>

      {mode === 'project' ? (
        <div
          style={{
            display: 'grid',
            gridTemplateColumns,
            gap: 12,
            alignItems: 'start',
          }}
        >
          <Card title={t('memory.contactListTitle')} bodyStyle={{ padding: 12 }}>
            <div style={{ maxHeight: listPaneHeight, overflowY: 'auto' }}>
              <List
                loading={loading && contacts.length === 0}
                dataSource={contacts}
                locale={{ emptyText: t('memory.noContacts') }}
                renderItem={(contact) => {
                  const selected = contact.id === selectedContactId;
                  return (
                    <List.Item style={{ border: 'none', padding: '0 0 10px' }}>
                      <div
                        style={{ width: '100%', ...getSelectableItemStyle(selected) }}
                        onClick={() => setSelectedContactId(contact.id)}
                      >
                        <Space direction="vertical" size={4} style={{ width: '100%' }}>
                          <Text strong style={{ color: selected ? '#0958d9' : undefined }}>
                            {getContactDisplayName(contact)}
                          </Text>
                          <Text type="secondary" ellipsis>
                            {contact.agent_id}
                          </Text>
                          <Text type="secondary">
                            {formatDateTime(contact.updated_at)}
                          </Text>
                        </Space>
                      </div>
                    </List.Item>
                  );
                }}
              />
            </div>
          </Card>

          <Card
            title={t('memory.projectListTitle')}
            extra={selectedContactId ? <Tag color="blue">{projectRows.length}</Tag> : null}
            bodyStyle={{ padding: 12 }}
          >
            {!selectedContactId ? (
              <Alert type="info" showIcon message={t('memory.selectContactHint')} />
            ) : (
              <div style={{ maxHeight: listPaneHeight, overflowY: 'auto' }}>
                <List
                  loading={loading && projectRows.length === 0}
                  dataSource={projectRows}
                  locale={{ emptyText: t('memory.emptyProject') }}
                  renderItem={(project) => {
                    const selected = project.project_id === selectedProjectId;
                    return (
                      <List.Item style={{ border: 'none', padding: '0 0 10px' }}>
                        <div
                          style={{ width: '100%', ...getSelectableItemStyle(selected) }}
                          onClick={() => setSelectedProjectId(project.project_id)}
                        >
                          <Space direction="vertical" size={6} style={{ width: '100%' }}>
                            <Text strong style={{ color: selected ? '#0958d9' : undefined }}>
                              {getProjectDisplayName(project)}
                            </Text>
                            {project.project_root && (
                              <Text type="secondary" ellipsis>
                                {project.project_root}
                              </Text>
                            )}
                            <Space size={8} wrap>
                              <Tag bordered={false}>V{project.memory_version ?? 0}</Tag>
                              <Tag bordered={false}>
                                {Number(project.recall_summarized) === 1
                                  ? t('memory.recallSummarized')
                                  : t('memory.projectSummaryTitle')}
                              </Tag>
                            </Space>
                            <Text type="secondary">
                              {formatDateTime(project.updated_at)}
                            </Text>
                          </Space>
                        </div>
                      </List.Item>
                    );
                  }}
                />
              </div>
            )}
          </Card>

          <Card
            title={t('memory.summaryListTitle')}
            extra={selectedProjectId ? <Tag color="geekblue">{summaryRows.length}</Tag> : null}
            bodyStyle={{ padding: 12 }}
          >
            {!selectedContactId ? (
              <Alert type="info" showIcon message={t('memory.selectContactHint')} />
            ) : !selectedProjectId ? (
              <Alert type="info" showIcon message={t('memory.selectProjectHint')} />
            ) : (
              <div style={{ maxHeight: listPaneHeight, overflowY: 'auto' }}>
                <Space direction="vertical" size={12} style={{ width: '100%' }}>
                  {selectedProject && (
                    <Card size="small" bodyStyle={{ padding: 12 }}>
                      <Space direction="vertical" size={4} style={{ width: '100%' }}>
                        <Text strong style={{ color: '#0958d9' }}>
                          {getProjectDisplayName(selectedProject)}
                        </Text>
                        {selectedProject.project_root && (
                          <Text type="secondary">{selectedProject.project_root}</Text>
                        )}
                        <Text type="secondary">
                          Session: {selectedSessionId || '-'}
                        </Text>
                      </Space>
                    </Card>
                  )}

                  {summaryRows.length === 0 ? (
                    <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('memory.emptyProject')} />
                  ) : (
                    <Table
                      rowKey="id"
                      size="small"
                      pagination={{ pageSize: 12, showSizeChanger: false }}
                      dataSource={summaryRows}
                      columns={summaryColumns}
                    />
                  )}
                </Space>
              </div>
            )}
          </Card>
        </div>
      ) : (
        <div
          style={{
            display: 'grid',
            gridTemplateColumns,
            gap: 12,
            alignItems: 'start',
          }}
        >
          <Card title={t('memory.contactListTitle')} bodyStyle={{ padding: 12 }}>
            <div style={{ maxHeight: listPaneHeight, overflowY: 'auto' }}>
              <List
                loading={loading && contacts.length === 0}
                dataSource={contacts}
                locale={{ emptyText: t('memory.noContacts') }}
                renderItem={(contact) => {
                  const selected = contact.id === selectedContactId;
                  return (
                    <List.Item style={{ border: 'none', padding: '0 0 10px' }}>
                      <div
                        style={{ width: '100%', ...getSelectableItemStyle(selected) }}
                        onClick={() => setSelectedContactId(contact.id)}
                      >
                        <Space direction="vertical" size={4} style={{ width: '100%' }}>
                          <Text strong style={{ color: selected ? '#0958d9' : undefined }}>
                            {getContactDisplayName(contact)}
                          </Text>
                          <Text type="secondary" ellipsis>
                            {contact.agent_id}
                          </Text>
                          <Text type="secondary">
                            {formatDateTime(contact.updated_at)}
                          </Text>
                        </Space>
                      </div>
                    </List.Item>
                  );
                }}
              />
            </div>
          </Card>

          <Card
            title={t('memory.recallListTitle')}
            extra={selectedContactId ? <Tag color="blue">{recallRows.length}</Tag> : null}
            bodyStyle={{ padding: 12 }}
          >
            {!selectedContactId ? (
              <Alert type="info" showIcon message={t('memory.selectContactHint')} />
            ) : (
              <div style={{ maxHeight: listPaneHeight, overflowY: 'auto' }}>
                <List
                  loading={loading && recallRows.length === 0}
                  dataSource={recallRows}
                  locale={{ emptyText: t('memory.emptyRecall') }}
                  renderItem={(recall) => {
                    const selected = recall.id === selectedRecallId;
                    const sourceProjects = recall.source_project_ids?.length
                      ? recall.source_project_ids.join(', ')
                      : '-';
                    return (
                      <List.Item style={{ border: 'none', padding: '0 0 10px' }}>
                        <div
                          style={{ width: '100%', ...getSelectableItemStyle(selected) }}
                          onClick={() => setSelectedRecallId(recall.id)}
                        >
                          <Space direction="vertical" size={6} style={{ width: '100%' }}>
                            <Space size={8} wrap>
                              <Tag color="blue">L{Number(recall.level) || 0}</Tag>
                              <Text strong style={{ color: selected ? '#0958d9' : undefined }}>
                                {recall.recall_key || '-'}
                              </Text>
                            </Space>
                            <Text type="secondary" ellipsis>
                              {t('memory.sourceProjects')}: {sourceProjects}
                            </Text>
                            <Space size={8} wrap>
                              <Tag bordered={false}>
                                {t('memory.updatedAt')}: {formatDateTime(recall.updated_at)}
                              </Tag>
                              {typeof recall.confidence === 'number' && (
                                <Tag bordered={false}>
                                  {t('memory.confidence')}: {recall.confidence.toFixed(2)}
                                </Tag>
                              )}
                            </Space>
                          </Space>
                        </div>
                      </List.Item>
                    );
                  }}
                />
              </div>
            )}
          </Card>

          <Card title={t('memory.recallDetailTitle')} bodyStyle={{ padding: 12 }}>
            {!selectedContactId ? (
              <Alert type="info" showIcon message={t('memory.selectContactHint')} />
            ) : !selectedRecall ? (
              <Alert type="info" showIcon message={t('memory.selectRecallHint')} />
            ) : (
              <div style={{ maxHeight: listPaneHeight, overflowY: 'auto' }}>
                <Space direction="vertical" size={12} style={{ width: '100%' }}>
                  <Card size="small" bodyStyle={{ padding: 12 }}>
                    <Space direction="vertical" size={6} style={{ width: '100%' }}>
                      <Text strong style={{ color: '#0958d9' }}>
                        {selectedContact ? getContactDisplayName(selectedContact) : '-'}
                      </Text>
                      <Space size={8} wrap>
                        <Tag color="blue">L{Number(selectedRecall.level) || 0}</Tag>
                        <Text code>{selectedRecall.recall_key || '-'}</Text>
                      </Space>
                      <Text type="secondary">
                        {t('memory.lastSeenAt')}: {formatDateTime(selectedRecall.last_seen_at)}
                      </Text>
                      <Text type="secondary">
                        {t('memory.updatedAt')}: {formatDateTime(selectedRecall.updated_at)}
                      </Text>
                      <Text type="secondary">
                        {t('memory.sourceProjects')}: {selectedRecall.source_project_ids?.length
                          ? selectedRecall.source_project_ids.join(', ')
                          : '-'}
                      </Text>
                      <Text type="secondary">
                        {t('memory.confidence')}: {typeof selectedRecall.confidence === 'number'
                          ? selectedRecall.confidence.toFixed(2)
                          : '-'}
                      </Text>
                    </Space>
                  </Card>
                  <Card size="small" title={t('memory.recallText')} bodyStyle={{ padding: 12 }}>
                    <Paragraph style={{ marginBottom: 0, whiteSpace: 'pre-wrap' }}>
                      {selectedRecall.recall_text || '-'}
                    </Paragraph>
                  </Card>
                </Space>
              </div>
            )}
          </Card>
        </div>
      )}
    </Space>
  );
}
