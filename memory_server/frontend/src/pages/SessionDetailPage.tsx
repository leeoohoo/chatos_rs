import { useEffect, useMemo, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Empty,
  Input,
  Space,
  Table,
  Tabs,
  Tag,
  Tree,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { DataNode } from 'antd/es/tree';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type {
  Message,
  SessionSummary,
  SummaryGraphEdge,
  SummaryGraphNode,
  SummaryLevelItem,
} from '../types';

interface SessionDetailPageProps {
  sessionId?: string;
  onBack?: () => void;
}

export function SessionDetailPage({ sessionId, onBack }: SessionDetailPageProps) {
  const { t } = useI18n();
  const [messages, setMessages] = useState<Message[]>([]);
  const [summaries, setSummaries] = useState<SessionSummary[]>([]);
  const [levelItems, setLevelItems] = useState<SummaryLevelItem[]>([]);
  const [levelNodes, setLevelNodes] = useState<SummaryGraphNode[]>([]);
  const [levelEdges, setLevelEdges] = useState<SummaryGraphEdge[]>([]);
  const [contextPreview, setContextPreview] = useState<string>('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [graphKeyword, setGraphKeyword] = useState('');
  const [expandedKeys, setExpandedKeys] = useState<string[]>([]);

  const load = async () => {
    if (!sessionId) {
      setMessages([]);
      setSummaries([]);
      setLevelItems([]);
      setLevelNodes([]);
      setLevelEdges([]);
      setContextPreview('');
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const [msg, sum, ctx, levels, graph] = await Promise.all([
        api.listMessages(sessionId),
        api.listSummaries(sessionId),
        api.composeContext(sessionId),
        api.listSummaryLevels(sessionId),
        api.getSummaryGraph(sessionId),
      ]);
      setMessages(msg);
      setSummaries(sum);
      setLevelItems(levels);
      setLevelNodes(graph.nodes);
      setLevelEdges(graph.edges);
      setContextPreview(ctx.merged_summary || t('sessionDetail.noSummary'));
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
      <Card title={t('sessionDetail.title')}>
        <Empty description={t('sessionDetail.pickFirst')} />
      </Card>
    );
  }

  const messageColumns: ColumnsType<Message> = [
    {
      title: t('sessionDetail.addRole'),
      dataIndex: 'role',
      key: 'role',
      width: 100,
      render: (value: string) => <Tag>{value}</Tag>,
    },
    {
      title: t('sessionDetail.addMessage'),
      dataIndex: 'content',
      key: 'content',
      ellipsis: true,
    },
    {
      title: t('sessions.status'),
      dataIndex: 'summary_status',
      key: 'summary_status',
      width: 130,
    },
    {
      title: t('sessionDetail.createdAt'),
      dataIndex: 'created_at',
      key: 'created_at',
      width: 220,
      render: (value: string) => new Date(value).toLocaleString(),
    },
  ];

  const summaryColumns: ColumnsType<SessionSummary> = [
    {
      title: t('summaryLevels.level'),
      dataIndex: 'level',
      key: 'level',
      width: 80,
    },
    {
      title: t('sessionDetail.summaries'),
      dataIndex: 'summary_text',
      key: 'summary_text',
      ellipsis: true,
    },
    {
      title: t('sessionDetail.sourceCount'),
      dataIndex: 'source_message_count',
      key: 'source_message_count',
      width: 120,
    },
    {
      title: t('summaryLevels.rollup'),
      dataIndex: 'rollup_status',
      key: 'rollup_status',
      width: 120,
    },
    {
      title: t('sessionDetail.createdAt'),
      dataIndex: 'created_at',
      key: 'created_at',
      width: 220,
      render: (value: string) => new Date(value).toLocaleString(),
    },
  ];

  const levelColumns: ColumnsType<SummaryLevelItem> = [
    { title: t('summaryLevels.level'), dataIndex: 'level', key: 'level', width: 100 },
    { title: t('summaryLevels.total'), dataIndex: 'total', key: 'total', width: 120 },
    { title: t('summaryLevels.pending'), dataIndex: 'pending', key: 'pending', width: 120 },
    { title: t('summaryLevels.summarized'), dataIndex: 'summarized', key: 'summarized', width: 140 },
  ];

  type GraphNodeRow = SummaryGraphNode & {
    label: string;
    parent_label: string;
    child_count: number;
  };

  type RelationRow = {
    key: string;
    from_id: string;
    to_id: string;
    from_level: number;
    to_level: number;
    from_label: string;
    to_label: string;
  };

  type SummaryTreeNode = DataNode & {
    search_text: string;
    children?: SummaryTreeNode[];
  };

  const makeReadableTitle = (excerpt: string | undefined, id: string, level: number): string => {
    const raw = (excerpt || '')
      .replace(/#{1,6}\s*/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();
    const short = raw.slice(0, 26);
    const suffix = raw.length > 26 ? '...' : '';
    return raw.length > 0 ? `Lv${level} · ${short}${suffix}` : `Lv${level} · ${id.slice(0, 8)}`;
  };

  const graphModel = useMemo((): {
    rootKeys: string[];
    allNodeKeys: string[];
    treeData: SummaryTreeNode[];
    nodeRows: GraphNodeRow[];
    relationRows: RelationRow[];
  } => {
    const sortedNodes = [...levelNodes].sort((a, b) => {
      if (a.level !== b.level) {
        return a.level - b.level;
      }
      if (a.created_at !== b.created_at) {
        return a.created_at.localeCompare(b.created_at);
      }
      return a.id.localeCompare(b.id);
    });

    const nodeMap = new Map<string, SummaryGraphNode>();
    sortedNodes.forEach((node) => nodeMap.set(node.id, node));

    const childrenMap = new Map<string, string[]>();
    const parentMap = new Map<string, string | null>();
    sortedNodes.forEach((node) => {
      const parentId =
        node.rollup_summary_id && nodeMap.has(node.rollup_summary_id)
          ? node.rollup_summary_id
          : null;
      parentMap.set(node.id, parentId);
      if (!parentId) {
        return;
      }
      const current = childrenMap.get(parentId) || [];
      current.push(node.id);
      childrenMap.set(parentId, current);
    });

    const nodeRows: GraphNodeRow[] = sortedNodes.map((node) => {
      const parentId = parentMap.get(node.id);
      return {
        ...node,
        label: makeReadableTitle(node.summary_excerpt, node.id, node.level),
        parent_label: parentId
          ? makeReadableTitle(nodeMap.get(parentId)?.summary_excerpt, parentId, nodeMap.get(parentId)?.level || 0)
          : '-',
        child_count: (childrenMap.get(node.id) || []).length,
      };
    });

    const relationRows: RelationRow[] = levelEdges
      .map((edge, idx) => {
        const fromNode = nodeMap.get(edge.from);
        const toNode = nodeMap.get(edge.to);
        if (!fromNode || !toNode) {
          return null;
        }
        return {
          key: `${edge.from}-${edge.to}-${idx}`,
          from_id: edge.from,
          to_id: edge.to,
          from_level: fromNode.level,
          to_level: toNode.level,
          from_label: makeReadableTitle(fromNode.summary_excerpt, edge.from, fromNode.level),
          to_label: makeReadableTitle(toNode.summary_excerpt, edge.to, toNode.level),
        };
      })
      .filter((item): item is RelationRow => item !== null);

    const roots = sortedNodes
      .filter((node) => !parentMap.get(node.id))
      .sort((a, b) => {
        if (a.level !== b.level) {
          return b.level - a.level;
        }
        return a.created_at.localeCompare(b.created_at);
      })
      .map((node) => node.id);

    const buildTreeNode = (id: string, stack: Set<string>): SummaryTreeNode => {
      const node = nodeMap.get(id);
      if (!node) {
        return {
          key: id,
          title: id.slice(0, 8),
          search_text: id,
        };
      }
      const title = makeReadableTitle(node.summary_excerpt, node.id, node.level);
      const nextStack = new Set(stack);
      nextStack.add(id);
      const childIds = (childrenMap.get(id) || []).filter((childId) => !nextStack.has(childId));
      const children = childIds.map((childId) => buildTreeNode(childId, nextStack));
      return {
        key: id,
        title: (
          <Space size={8}>
            <Tag color="blue">{`L${node.level}`}</Tag>
            <span>{title}</span>
            <Typography.Text type="secondary">{`#${id.slice(0, 8)}`}</Typography.Text>
          </Space>
        ),
        search_text: `${title} ${id.slice(0, 8)} ${node.level}`,
        children,
      };
    };

    const treeData = roots.map((id) => buildTreeNode(id, new Set<string>()));
    return {
      rootKeys: roots,
      allNodeKeys: sortedNodes.map((node) => node.id),
      treeData,
      nodeRows,
      relationRows,
    };
  }, [levelNodes, levelEdges]);

  useEffect(() => {
    setExpandedKeys(graphModel.rootKeys);
  }, [graphModel.rootKeys]);

  useEffect(() => {
    if (graphKeyword.trim()) {
      setExpandedKeys(graphModel.allNodeKeys);
    }
  }, [graphKeyword, graphModel.allNodeKeys]);

  const filteredTreeData = useMemo(() => {
    const keyword = graphKeyword.trim().toLowerCase();
    if (!keyword) {
      return graphModel.treeData;
    }

    const filterNode = (node: SummaryTreeNode): SummaryTreeNode | null => {
      const children = (node.children || [])
        .map((child) => filterNode(child as SummaryTreeNode))
        .filter((item): item is SummaryTreeNode => item !== null);
      const matched = node.search_text.toLowerCase().includes(keyword);
      if (!matched && children.length === 0) {
        return null;
      }
      return {
        ...node,
        children,
      };
    };

    return graphModel.treeData
      .map((item) => filterNode(item))
      .filter((item): item is SummaryTreeNode => item !== null);
  }, [graphKeyword, graphModel.treeData]);

  const nodeColumns: ColumnsType<GraphNodeRow> = [
    { title: t('summaryLevels.nodeLabel'), dataIndex: 'label', key: 'label', width: 260, ellipsis: true },
    { title: t('summaryLevels.level'), dataIndex: 'level', key: 'level', width: 80 },
    { title: t('summaryLevels.status'), dataIndex: 'status', key: 'status', width: 120 },
    { title: t('summaryLevels.rollup'), dataIndex: 'rollup_status', key: 'rollup_status', width: 130 },
    { title: t('summaryLevels.parentNode'), dataIndex: 'parent_label', key: 'parent_label', width: 260, ellipsis: true },
    { title: t('summaryLevels.childCount'), dataIndex: 'child_count', key: 'child_count', width: 100 },
    { title: t('summaryLevels.idRef'), dataIndex: 'id', key: 'id', width: 120, render: (value: string) => value.slice(0, 8) },
    { title: t('summaryLevels.excerpt'), dataIndex: 'summary_excerpt', key: 'summary_excerpt', ellipsis: true },
  ];

  const relationColumns: ColumnsType<RelationRow> = [
    { title: t('summaryLevels.from'), dataIndex: 'from_label', key: 'from_label', ellipsis: true, width: 320 },
    { title: t('summaryLevels.to'), dataIndex: 'to_label', key: 'to_label', ellipsis: true, width: 320 },
    { title: t('summaryLevels.level'), dataIndex: 'from_level', key: 'from_level', width: 90 },
    { title: t('summaryLevels.parentLevel'), dataIndex: 'to_level', key: 'to_level', width: 110 },
    { title: t('summaryLevels.idRef'), dataIndex: 'from_id', key: 'from_id', width: 120, render: (value: string) => value.slice(0, 8) },
  ];

  const content = (
    <>
      <Space style={{ width: '100%', justifyContent: 'space-between', marginBottom: 12 }}>
        <Space>
          {onBack && <Button onClick={onBack}>{t('common.back')}</Button>}
          <Typography.Paragraph type="secondary" style={{ marginBottom: 0 }}>
            {t('sessionDetail.sessionLabel')}: {sessionId}
          </Typography.Paragraph>
        </Space>
        <Button onClick={load} loading={loading}>
          {t('common.refresh')}
        </Button>
      </Space>

      {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}

      <Tabs
        items={[
          {
            key: 'messages',
            label: `${t('sessionDetail.messages')} (${messages.length})`,
            children: (
              <Table
                rowKey="id"
                loading={loading}
                columns={messageColumns}
                dataSource={messages}
                pagination={{ pageSize: 10 }}
                size="small"
              />
            ),
          },
          {
            key: 'summaries',
            label: `${t('sessionDetail.summaries')} (${summaries.length})`,
            children: (
              <Table
                rowKey="id"
                loading={loading}
                columns={summaryColumns}
                dataSource={summaries}
                pagination={{ pageSize: 10 }}
                size="small"
              />
            ),
          },
          {
            key: 'context',
            label: t('sessionDetail.context'),
            children: (
              <Typography.Paragraph className="memory-context-box">
                {contextPreview}
              </Typography.Paragraph>
            ),
          },
          {
            key: 'levels',
            label: t('summaryLevels.title'),
            children: (
              <Space direction="vertical" size={12} style={{ width: '100%' }}>
                <Card size="small" title={`${t('summaryLevels.level')} / ${t('summaryLevels.total')}`}>
                  <Table
                    rowKey="level"
                    loading={loading}
                    columns={levelColumns}
                    dataSource={levelItems}
                    pagination={false}
                    size="small"
                  />
                </Card>
                <Card size="small" title={`${t('summaryLevels.treeView')} (${graphModel.rootKeys.length})`}>
                  <Typography.Paragraph type="secondary" style={{ marginBottom: 10 }}>
                    {t('summaryLevels.graphHint')}
                  </Typography.Paragraph>
                  <Input
                    value={graphKeyword}
                    onChange={(e) => setGraphKeyword(e.target.value)}
                    placeholder={t('summaryLevels.searchNodePlaceholder')}
                    style={{ marginBottom: 10, maxWidth: 360 }}
                    allowClear
                  />
                  <div className="memory-tree-wrap">
                    <Tree
                      showLine
                      blockNode
                      height={440}
                      treeData={filteredTreeData}
                      expandedKeys={expandedKeys}
                      onExpand={(keys) => setExpandedKeys(keys as string[])}
                    />
                  </div>
                </Card>
                <Card size="small" title={`${t('summaryLevels.nodes')} (${graphModel.nodeRows.length})`}>
                  <Table
                    rowKey="id"
                    loading={loading}
                    columns={nodeColumns}
                    dataSource={graphModel.nodeRows}
                    pagination={{ pageSize: 8 }}
                    size="small"
                  />
                </Card>
                <Card size="small" title={`${t('summaryLevels.relationList')} (${graphModel.relationRows.length})`}>
                  <Table
                    rowKey="key"
                    loading={loading}
                    columns={relationColumns}
                    dataSource={graphModel.relationRows}
                    pagination={{ pageSize: 12 }}
                    size="small"
                  />
                </Card>
              </Space>
            ),
          },
        ]}
      />
    </>
  );

  return <Card title={t('sessionDetail.title')}>{content}</Card>;
}
