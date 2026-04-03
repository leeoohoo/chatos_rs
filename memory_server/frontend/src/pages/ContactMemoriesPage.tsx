import { useEffect, useMemo, useState } from 'react';
import { Alert, Button, Card, Descriptions, Empty, Grid, List, Modal, Space, Table, Tabs, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type {
  AgentRecall,
  ContactProject,
  MemoryContact,
  SessionSummary,
  TaskExecutionSummary,
} from '../types';

const { Text, Paragraph } = Typography;
const { useBreakpoint } = Grid;

interface ContactMemoriesPageProps {
  filterUserId?: string;
  currentUserId: string;
  isAdmin: boolean;
  mode: 'project' | 'recall';
}

type SummaryDetailState =
  | { kind: 'session'; item: SessionSummary }
  | { kind: 'task'; item: TaskExecutionSummary }
  | null;

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

function buildSummaryPreview(value?: string | null): string {
  const text = typeof value === 'string' ? value.trim() : '';
  if (!text) {
    return '-';
  }
  const singleLine = text.replace(/\s+/g, ' ').trim();
  return singleLine.length > 90 ? `${singleLine.slice(0, 90)}...` : singleLine;
}

function formatTriggerType(value?: string | null): string {
  const raw = typeof value === 'string' ? value.trim() : '';
  if (!raw) {
    return '-';
  }

  const labels: string[] = [];
  if (raw.includes('message_count_limit')) {
    labels.push('达到批次阈值');
  }
  if (raw.includes('oversized_single_skipped')) {
    labels.push('跳过超长单条');
  }
  if (raw.includes('overflow_retry')) {
    labels.push('超限后重试');
  }
  if (raw.includes('forced_truncated')) {
    labels.push('强制截断');
  }

  return labels.length > 0 ? labels.join(' / ') : raw;
}

function buildRecallPreview(value?: string | null): string {
  const text = typeof value === 'string' ? value.trim() : '';
  if (!text) {
    return '-';
  }
  return text.length > 120 ? `${text.slice(0, 120)}...` : text;
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
  const [taskExecutionSummaries, setTaskExecutionSummaries] = useState<TaskExecutionSummary[]>([]);
  const [agentRecalls, setAgentRecalls] = useState<AgentRecall[]>([]);
  const [selectedRecallId, setSelectedRecallId] = useState<string | undefined>(undefined);
  const [summaryTab, setSummaryTab] = useState<'session' | 'task'>('session');
  const [summaryDetail, setSummaryDetail] = useState<SummaryDetailState>(null);
  const [recallDetail, setRecallDetail] = useState<AgentRecall | null>(null);
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

  const taskSummaryRows = useMemo(() => {
    return sortByCreatedDesc(taskExecutionSummaries);
  }, [taskExecutionSummaries]);

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
      setTaskExecutionSummaries([]);
      return;
    }
    const nextProjectId = selectedProjectId && sorted.some((item) => item.project_id === selectedProjectId)
      ? selectedProjectId
      : sorted[0].project_id;
    setSelectedProjectId(nextProjectId);
    if (!nextProjectId) {
      setSelectedSessionId(undefined);
      setSessionSummaries([]);
      setTaskExecutionSummaries([]);
    }
  };

  const loadProjectSummaryDetail = async (
    contactId: string,
    projectId: string,
    contactAgentId: string,
  ) => {
    const pid = projectId.trim();
    if (!pid) {
      setSelectedSessionId(undefined);
      setSessionSummaries([]);
      setTaskExecutionSummaries([]);
      return;
    }
    const [sessionData, taskItems] = await Promise.all([
      api.listContactProjectSummaries(contactId, pid),
      api.listTaskExecutionSummaries(scopeUserId, contactAgentId, pid),
    ]);
    setSelectedSessionId(sessionData.session_id ?? undefined);
    setSessionSummaries(sortByCreatedDesc(sessionData.items));
    setTaskExecutionSummaries(sortByCreatedDesc(taskItems));
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
      setTaskExecutionSummaries([]);
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
    if (mode !== 'project' || !selectedContactId || !selectedProjectId || !selectedContact?.agent_id) {
      return;
    }
    const run = async () => {
      try {
        await loadProjectSummaryDetail(selectedContactId, selectedProjectId, selectedContact.agent_id);
      } catch (err) {
        setError((err as Error).message);
      }
    };
    void run();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, selectedContact?.agent_id, selectedContactId, selectedProjectId, scopeUserId]);

  const summaryColumns: ColumnsType<SessionSummary> = [
    {
      title: '摘要内容',
      dataIndex: 'summary_text',
      key: 'summary_text',
      render: (value?: string) => (
        <Paragraph style={{ marginBottom: 0 }} ellipsis={{ rows: 2, tooltip: value || '-' }}>
          {buildSummaryPreview(value)}
        </Paragraph>
      ),
    },
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
      title: '详情',
      key: 'action',
      width: 92,
      fixed: 'right',
      render: (_: unknown, record) => (
        <Button type="link" size="small" onClick={() => setSummaryDetail({ kind: 'session', item: record })}>
          详情
        </Button>
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

  const taskSummaryColumns: ColumnsType<TaskExecutionSummary> = [
    {
      title: '摘要内容',
      dataIndex: 'summary_text',
      key: 'summary_text',
      render: (value?: string) => (
        <Paragraph style={{ marginBottom: 0 }} ellipsis={{ rows: 2, tooltip: value || '-' }}>
          {buildSummaryPreview(value)}
        </Paragraph>
      ),
    },
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
      render: (value?: string, record?: TaskExecutionSummary) => {
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
      title: '详情',
      key: 'action',
      width: 92,
      fixed: 'right',
      render: (_: unknown, record) => (
        <Button type="link" size="small" onClick={() => setSummaryDetail({ kind: 'task', item: record })}>
          详情
        </Button>
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

  const getRecallDirectSourceLabel = (level?: number): string => {
    return Number(level) > 0
      ? t('memory.directSourceLowerRecall')
      : t('memory.directSourceProjectSummary');
  };

  const listPaneHeight = screens.lg ? 620 : undefined;
  const gridTemplateColumns = screens.xl
    ? '280px 320px minmax(0, 1fr)'
    : screens.lg
      ? '240px 280px minmax(0, 1fr)'
      : '1fr';
  const recallGridTemplateColumns = screens.xl
    ? '280px minmax(0, 1fr)'
    : screens.lg
      ? '240px minmax(0, 1fr)'
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

  const summaryDetailTitle = useMemo(() => {
    if (!summaryDetail) {
      return '';
    }
    return summaryDetail.kind === 'session' ? '会话总结详情' : '任务执行总结详情';
  }, [summaryDetail]);

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
            title={t('memory.projectContextSummary')}
            extra={selectedProjectId ? <Tag color="geekblue">{summaryRows.length + taskSummaryRows.length}</Tag> : null}
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
                        <Space size={8} wrap>
                          <Tag color="blue">
                            {t('memory.sessionSummaryTab')}: {summaryRows.length}
                          </Tag>
                          <Tag color="geekblue">
                            {t('memory.taskExecutionSummaryTab')}: {taskSummaryRows.length}
                          </Tag>
                        </Space>
                      </Space>
                    </Card>
                  )}

                  <Tabs
                    activeKey={summaryTab}
                    onChange={(value) => setSummaryTab(value as 'session' | 'task')}
                    items={[
                      {
                        key: 'session',
                        label: (
                          <Space size={6}>
                            <span>{t('memory.sessionSummaryTab')}</span>
                            <Tag color="blue" bordered={false}>{summaryRows.length}</Tag>
                          </Space>
                        ),
                      },
                      {
                        key: 'task',
                        label: (
                          <Space size={6}>
                            <span>{t('memory.taskExecutionSummaryTab')}</span>
                            <Tag color="geekblue" bordered={false}>{taskSummaryRows.length}</Tag>
                          </Space>
                        ),
                      },
                    ]}
                  />

                  {summaryTab === 'session' ? (
                    summaryRows.length === 0 ? (
                      <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('memory.emptyProject')} />
                    ) : (
                      <Table
                        rowKey="id"
                        size="small"
                        pagination={{ pageSize: 12, showSizeChanger: false }}
                        dataSource={summaryRows}
                        columns={summaryColumns}
                        scroll={{ x: 980 }}
                      />
                    )
                  ) : (
                    taskSummaryRows.length === 0 ? (
                      <Empty
                        image={Empty.PRESENTED_IMAGE_SIMPLE}
                        description={t('memory.emptyTaskExecutionSummary')}
                      />
                    ) : (
                      <Table
                        rowKey="id"
                        size="small"
                        pagination={{ pageSize: 12, showSizeChanger: false }}
                        dataSource={taskSummaryRows}
                        columns={taskSummaryColumns}
                        scroll={{ x: 980 }}
                      />
                    )
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
            gridTemplateColumns: recallGridTemplateColumns,
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
                    return (
                      <List.Item style={{ border: 'none', padding: '0 0 10px' }}>
                        <div
                          style={{ width: '100%', ...getSelectableItemStyle(selected) }}
                          onClick={() => setSelectedRecallId(recall.id)}
                        >
                          <Space direction="vertical" size={6} style={{ width: '100%' }}>
                            <Space size={8} wrap>
                              <Tag color="blue">L{Number(recall.level) || 0}</Tag>
                              <Tag bordered={false}>
                                {t('memory.directSource')}: {getRecallDirectSourceLabel(recall.level)}
                              </Tag>
                              <Text strong style={{ color: selected ? '#0958d9' : undefined }}>
                                {recall.recall_key || '-'}
                              </Text>
                            </Space>
                            <Paragraph
                              type="secondary"
                              style={{ marginBottom: 0 }}
                              ellipsis={{ rows: 3, tooltip: recall.recall_text || '-' }}
                            >
                              {buildRecallPreview(recall.recall_text)}
                            </Paragraph>
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
                            <div>
                              <Button
                                type="link"
                                size="small"
                                style={{ paddingLeft: 0 }}
                                onClick={(event) => {
                                  event.stopPropagation();
                                  setSelectedRecallId(recall.id);
                                  setRecallDetail(recall);
                                }}
                              >
                                {t('common.detail')}
                              </Button>
                            </div>
                          </Space>
                        </div>
                      </List.Item>
                    );
                  }}
                />
              </div>
            )}
          </Card>
        </div>
      )}

      <Modal
        open={Boolean(summaryDetail)}
        title={summaryDetailTitle}
        footer={null}
        onCancel={() => setSummaryDetail(null)}
        width={860}
      >
        {summaryDetail && (
          <Space direction="vertical" size={16} style={{ width: '100%' }}>
            <Descriptions bordered size="small" column={2}>
              <Descriptions.Item label="类型">
                {summaryDetail.kind === 'session' ? t('memory.sessionSummaryTab') : t('memory.taskExecutionSummaryTab')}
              </Descriptions.Item>
              <Descriptions.Item label={t('summaryLevels.level')}>
                <Tag color="blue">L{Number(summaryDetail.item.level) || 0}</Tag>
              </Descriptions.Item>
              <Descriptions.Item label={t('summaryLevels.status')}>
                <Tag color={summaryDetail.item.status === 'summarized' || summaryDetail.item.rollup_summary_id ? 'default' : 'processing'}>
                  {summaryDetail.item.status || 'pending'}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label={t('memory.agentMemoryStatus')}>
                <Tag color={Number(summaryDetail.item.agent_memory_summarized) === 1 ? 'green' : 'default'}>
                  {Number(summaryDetail.item.agent_memory_summarized) === 1 ? t('memory.agentMemoryDone') : t('memory.agentMemoryPending')}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="触发原因" span={2}>
                {formatTriggerType(summaryDetail.item.trigger_type)}
              </Descriptions.Item>
              {summaryDetail.kind === 'task' && (
                <Descriptions.Item label={t('memory.scopeKey')} span={2}>
                  <Text code copyable>{summaryDetail.item.scope_key || '-'}</Text>
                </Descriptions.Item>
              )}
              {'session_id' in summaryDetail.item && (
                <Descriptions.Item label="Session ID" span={2}>
                  <Text code copyable>{summaryDetail.item.session_id || '-'}</Text>
                </Descriptions.Item>
              )}
              <Descriptions.Item label={t('sessionDetail.sourceCount')}>
                {Number(summaryDetail.item.source_message_count) || 0}
              </Descriptions.Item>
              <Descriptions.Item label={t('memory.updatedAt')}>
                {formatDateTime(summaryDetail.item.created_at)}
              </Descriptions.Item>
              <Descriptions.Item label={t('memory.rollupTarget')}>
                {summaryDetail.item.rollup_summary_id ? <Text code copyable>{summaryDetail.item.rollup_summary_id}</Text> : '-'}
              </Descriptions.Item>
              <Descriptions.Item label="模型">
                {summaryDetail.item.summary_model || '-'}
              </Descriptions.Item>
            </Descriptions>

            <Card size="small" title="总结内容">
              <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                {summaryDetail.item.summary_text || '-'}
              </Paragraph>
            </Card>
          </Space>
        )}
      </Modal>

      <Modal
        open={Boolean(recallDetail)}
        title={t('memory.recallDetailTitle')}
        footer={null}
        onCancel={() => setRecallDetail(null)}
        width={860}
      >
        {recallDetail && (
          <Space direction="vertical" size={16} style={{ width: '100%' }}>
            <Descriptions bordered size="small" column={2}>
              <Descriptions.Item label={t('memory.contact')}>
                {selectedContact ? getContactDisplayName(selectedContact) : '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('memory.recallLevel')}>
                <Tag color="blue">L{Number(recallDetail.level) || 0}</Tag>
              </Descriptions.Item>
              <Descriptions.Item label={t('memory.directSource')}>
                {getRecallDirectSourceLabel(recallDetail.level)}
              </Descriptions.Item>
              <Descriptions.Item label={t('memory.confidence')}>
                {typeof recallDetail.confidence === 'number' ? recallDetail.confidence.toFixed(2) : '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('memory.recallKey')} span={2}>
                <Text code copyable>{recallDetail.recall_key || '-'}</Text>
              </Descriptions.Item>
              <Descriptions.Item label={t('memory.lastSeenAt')}>
                {formatDateTime(recallDetail.last_seen_at)}
              </Descriptions.Item>
              <Descriptions.Item label={t('memory.updatedAt')}>
                {formatDateTime(recallDetail.updated_at)}
              </Descriptions.Item>
            </Descriptions>

            <Card size="small" title={t('memory.recallText')}>
              <Paragraph style={{ marginBottom: 0, whiteSpace: 'pre-wrap' }}>
                {recallDetail.recall_text || '-'}
              </Paragraph>
            </Card>
          </Space>
        )}
      </Modal>
    </Space>
  );
}
