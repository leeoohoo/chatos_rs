import { useEffect, useState } from 'react';
import { Alert, Button, Card, Empty, Space, Table, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { SummaryGraphEdge, SummaryGraphNode, SummaryLevelItem } from '../types';

interface SummaryLevelsPageProps {
  sessionId?: string;
}

export function SummaryLevelsPage({ sessionId }: SummaryLevelsPageProps) {
  const { t } = useI18n();
  const [items, setItems] = useState<SummaryLevelItem[]>([]);
  const [nodes, setNodes] = useState<SummaryGraphNode[]>([]);
  const [edges, setEdges] = useState<SummaryGraphEdge[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    if (!sessionId) {
      setItems([]);
      setNodes([]);
      setEdges([]);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const [levels, graph] = await Promise.all([
        api.listSummaryLevels(sessionId),
        api.getSummaryGraph(sessionId),
      ]);
      setItems(levels);
      setNodes(graph.nodes);
      setEdges(graph.edges);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  if (!sessionId) {
    return (
      <Card title={t('summaryLevels.title')}>
        <Empty description={t('summaryLevels.pickFirst')} />
      </Card>
    );
  }

  const levelColumns: ColumnsType<SummaryLevelItem> = [
    { title: t('summaryLevels.level'), dataIndex: 'level', key: 'level', width: 100 },
    { title: t('summaryLevels.total'), dataIndex: 'total', key: 'total', width: 120 },
    { title: t('summaryLevels.pending'), dataIndex: 'pending', key: 'pending', width: 120 },
    { title: t('summaryLevels.summarized'), dataIndex: 'summarized', key: 'summarized', width: 140 },
  ];

  const nodeColumns: ColumnsType<SummaryGraphNode> = [
    {
      title: t('sessions.id'),
      dataIndex: 'id',
      key: 'id',
      width: 120,
      render: (value: string) => value.slice(0, 8),
    },
    { title: t('summaryLevels.level'), dataIndex: 'level', key: 'level', width: 80 },
    { title: t('summaryLevels.status'), dataIndex: 'status', key: 'status', width: 120 },
    {
      title: t('summaryLevels.parent'),
      dataIndex: 'rollup_summary_id',
      key: 'rollup_summary_id',
      width: 120,
      render: (value?: string | null) => (value ? value.slice(0, 8) : '-'),
    },
    { title: t('summaryLevels.excerpt'), dataIndex: 'summary_excerpt', key: 'summary_excerpt', ellipsis: true },
  ];

  const edgeColumns: ColumnsType<SummaryGraphEdge> = [
    {
      title: t('summaryLevels.from'),
      dataIndex: 'from',
      key: 'from',
      render: (value: string) => value.slice(0, 8),
    },
    {
      title: t('summaryLevels.to'),
      dataIndex: 'to',
      key: 'to',
      render: (value: string) => value.slice(0, 8),
    },
  ];

  return (
    <Space direction="vertical" size={12} style={{ width: '100%' }}>
      <Card
        title={t('summaryLevels.title')}
        extra={
          <Button onClick={load} loading={loading}>
            {t('common.refresh')}
          </Button>
        }
      >
        <Typography.Paragraph type="secondary" style={{ marginBottom: 12 }}>
          {t('summaryLevels.sessionLabel')}: {sessionId}
        </Typography.Paragraph>
        {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
        <Table
          rowKey="level"
          loading={loading}
          columns={levelColumns}
          dataSource={items}
          pagination={false}
          size="small"
        />
      </Card>

      <Card title={`${t('summaryLevels.nodes')} (${nodes.length})`}>
        <Table
          rowKey="id"
          loading={loading}
          columns={nodeColumns}
          dataSource={nodes}
          pagination={{ pageSize: 8 }}
          size="small"
        />
      </Card>

      <Card title={`${t('summaryLevels.edges')} (${edges.length})`}>
        <Table
          rowKey={(item) => `${item.from}-${item.to}`}
          loading={loading}
          columns={edgeColumns}
          dataSource={edges}
          pagination={{ pageSize: 8 }}
          size="small"
        />
      </Card>
    </Space>
  );
}
