import { Card, Empty, Space, Table, Tabs, Tag, Typography } from 'antd';
import type { TableColumnsType, TabsProps } from 'antd';

import type { EngineRecord, EngineSubjectMemory, EngineSummary, EngineThread } from '../../../types';
import {
  formatStructuredText,
  statusColor,
  threadDisplayName,
  threadScopeKey,
  toLocal,
} from '../../utils';
import type { DataDetailTab, ThreadWorkspaceProps, UserLabelMap } from './types';

const { Paragraph, Text } = Typography;

function shortText(value?: string | null): string {
  const trimmed = value?.trim();
  return trimmed ? trimmed : '-';
}

function statusTag(value?: string | null) {
  const text = shortText(value);
  return <Tag color={statusColor(text)}>{text}</Tag>;
}

function tenantDisplayText(tenantId: string, tenantLabelsById: UserLabelMap): string {
  const user = tenantLabelsById[tenantId];
  if (!user) {
    return tenantId;
  }
  const username = user.username.trim();
  const displayName = user.display_name.trim();
  const label = displayName || username || tenantId;
  return username && username !== label ? `${label} (${username})` : label;
}

function jsonBlock(value: unknown) {
  const text = formatStructuredText(value);
  if (!text) {
    return <Text type="secondary">无</Text>;
  }
  return <pre className="engine-record-modal-pre">{text}</pre>;
}

function threadColumns(
  selectedThread: EngineThread | null,
  tenantLabelsById: UserLabelMap,
): TableColumnsType<EngineThread> {
  const selectedKey = threadScopeKey(selectedThread);
  return [
    {
      title: '线程',
      key: 'thread',
      width: 270,
      render: (_value, record) => (
        <div className="engine-thread-cell">
          <Text strong className="engine-thread-title" ellipsis>
            {threadDisplayName(record)}
          </Text>
          <Text className="engine-thread-id" type="secondary" copyable>
            {record.id}
          </Text>
        </div>
      ),
    },
    {
      title: '租户 / 来源',
      key: 'scope',
      width: 220,
      render: (_value, record) => (
        <Space direction="vertical" size={2}>
          <Text>{tenantDisplayText(record.tenant_id, tenantLabelsById)}</Text>
          <Text type="secondary">{record.source_id}</Text>
        </Space>
      ),
    },
    {
      title: '状态',
      key: 'status',
      width: 120,
      render: (_value, record) => (
        <Space direction="vertical" size={2}>
          {statusTag(record.status)}
          <Tag>{record.summary_status}</Tag>
        </Space>
      ),
    },
    {
      title: '待处理',
      key: 'pending',
      width: 100,
      align: 'right',
      render: (_value, record) => record.pending_record_count,
    },
    {
      title: '更新',
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 180,
      render: toLocal,
    },
    {
      title: '',
      key: 'selected',
      width: 1,
      render: (_value, record) =>
        threadScopeKey(record) === selectedKey ? <span aria-label="selected" /> : null,
    },
  ];
}

const recordColumns: TableColumnsType<EngineRecord> = [
  {
    title: '角色',
    dataIndex: 'role',
    key: 'role',
    width: 100,
    render: (value: string) => <Tag>{value}</Tag>,
  },
  {
    title: '类型',
    dataIndex: 'record_type',
    key: 'record_type',
    width: 120,
  },
  {
    title: '内容',
    dataIndex: 'content',
    key: 'content',
    render: (value?: string | null) => (
      <Paragraph ellipsis={{ rows: 2 }} style={{ marginBottom: 0 }}>
        {shortText(value)}
      </Paragraph>
    ),
  },
  {
    title: '总结状态',
    dataIndex: 'summary_status',
    key: 'summary_status',
    width: 120,
    render: (value?: string | null) => <Tag>{shortText(value)}</Tag>,
  },
  {
    title: '创建',
    dataIndex: 'created_at',
    key: 'created_at',
    width: 180,
    render: toLocal,
  },
];

const summaryColumns: TableColumnsType<EngineSummary> = [
  {
    title: '类型',
    dataIndex: 'summary_type',
    key: 'summary_type',
    width: 170,
  },
  {
    title: '层级',
    dataIndex: 'level',
    key: 'level',
    width: 80,
    align: 'right',
  },
  {
    title: '内容',
    dataIndex: 'summary_text',
    key: 'summary_text',
    render: (value?: string | null) => (
      <Paragraph ellipsis={{ rows: 3 }} style={{ marginBottom: 0 }}>
        {shortText(value)}
      </Paragraph>
    ),
  },
  {
    title: '状态',
    dataIndex: 'status',
    key: 'status',
    width: 110,
    render: statusTag,
  },
  {
    title: '创建',
    dataIndex: 'created_at',
    key: 'created_at',
    width: 180,
    render: toLocal,
  },
];

const memoryColumns: TableColumnsType<EngineSubjectMemory> = [
  {
    title: '记忆键',
    dataIndex: 'memory_key',
    key: 'memory_key',
    width: 180,
    render: (value?: string | null) => <Text copyable>{shortText(value)}</Text>,
  },
  {
    title: '类型',
    dataIndex: 'memory_type',
    key: 'memory_type',
    width: 120,
  },
  {
    title: '层级',
    dataIndex: 'level',
    key: 'level',
    width: 80,
    align: 'right',
  },
  {
    title: '内容',
    dataIndex: 'text',
    key: 'text',
    render: (value?: string | null) => (
      <Paragraph ellipsis={{ rows: 3 }} style={{ marginBottom: 0 }}>
        {shortText(value)}
      </Paragraph>
    ),
  },
  {
    title: '状态',
    dataIndex: 'status',
    key: 'status',
    width: 110,
    render: statusTag,
  },
  {
    title: '更新',
    dataIndex: 'updated_at',
    key: 'updated_at',
    width: 180,
    render: toLocal,
  },
];

function recordsTable(props: ThreadWorkspaceProps) {
  return (
    <div className="engine-data-tab-pane">
      <Table<EngineRecord>
        className="engine-fill-table"
        rowKey="id"
        dataSource={props.threadRecords}
        loading={props.threadRecordsLoading}
        columns={recordColumns}
        scroll={{ x: 980, y: 'calc(100vh - 430px)' }}
        pagination={{
          current: props.threadRecordPage,
          pageSize: props.threadRecordPageSize,
          total: props.threadRecordTotal,
          showSizeChanger: true,
          onChange: props.onThreadRecordPageChange,
        }}
        expandable={{
          expandedRowRender: (record) => (
            <Space direction="vertical" size={12} style={{ width: '100%' }}>
              <div>
                <Text strong>内容</Text>
                {jsonBlock(record.content)}
              </div>
              <div>
                <Text strong>结构化负载</Text>
                {jsonBlock(record.structured_payload)}
              </div>
              <div>
                <Text strong>元数据</Text>
                {jsonBlock(record.metadata)}
              </div>
            </Space>
          ),
        }}
      />
    </div>
  );
}

function summariesTable(props: ThreadWorkspaceProps) {
  return (
    <div className="engine-data-tab-pane">
      <Table<EngineSummary>
        className="engine-fill-table"
        rowKey="id"
        dataSource={props.threadSummaries}
        loading={props.threadDetailLoading}
        columns={summaryColumns}
        pagination={{ pageSize: 10 }}
        scroll={{ x: 960, y: 'calc(100vh - 430px)' }}
        expandable={{
          expandedRowRender: (summary) => jsonBlock(summary),
        }}
      />
    </div>
  );
}

function memoriesTable(props: ThreadWorkspaceProps) {
  return (
    <div className="engine-data-tab-pane">
      <Table<EngineSubjectMemory>
        className="engine-fill-table"
        rowKey="id"
        dataSource={props.subjectMemories}
        loading={props.threadDetailLoading}
        columns={memoryColumns}
        pagination={{ pageSize: 10 }}
        scroll={{ x: 980, y: 'calc(100vh - 430px)' }}
        expandable={{
          expandedRowRender: (memory) => jsonBlock(memory),
        }}
      />
    </div>
  );
}

export function ThreadWorkspace(props: ThreadWorkspaceProps) {
  const {
    threads,
    threadsLoading,
    selectedThread,
    onSelectThread,
    detailTab,
    onDetailTabChange,
  } = props;

  const detailItems: TabsProps['items'] = [
    {
      key: 'records',
      label: `记录 (${props.threadRecordTotal})`,
      children: recordsTable(props),
    },
    {
      key: 'summaries',
      label: `总结 (${props.threadSummaries.length})`,
      children: summariesTable(props),
    },
    {
      key: 'memories',
      label: `主题记忆 (${props.subjectMemories.length})`,
      children: memoriesTable(props),
    },
  ];

  return (
    <div className="engine-data-workspace">
      <div className="engine-data-column">
        <Card className="engine-data-card" title={`线程 (${threads.length})`}>
          <div className="engine-data-table-shell">
            <Table<EngineThread>
              className="engine-fill-table"
              rowKey={(record) => threadScopeKey(record) ?? record.id}
              dataSource={threads}
              loading={threadsLoading}
              columns={threadColumns(selectedThread, props.tenantLabelsById ?? {})}
              pagination={{ pageSize: 20 }}
              scroll={{ x: 880, y: 'calc(100vh - 390px)' }}
              rowClassName={(record) =>
                threadScopeKey(record) === threadScopeKey(selectedThread)
                  ? 'engine-thread-row engine-thread-row--selected'
                  : 'engine-thread-row'
              }
              onRow={(record) => ({
                onClick: () => onSelectThread(record),
              })}
            />
          </div>
        </Card>
      </div>
      <div className="engine-data-column">
        {selectedThread ? (
          <Card
            className="engine-data-card"
            title={threadDisplayName(selectedThread)}
            extra={<Text type="secondary">{selectedThread.id}</Text>}
          >
            <Tabs
              className="engine-data-detail-tabs"
              activeKey={detailTab}
              onChange={(key) => onDetailTabChange(key as DataDetailTab)}
              items={detailItems}
            />
          </Card>
        ) : (
          <Card className="engine-data-card engine-data-card--empty">
            <div className="engine-data-empty">
              <Empty description="选择一个线程查看详情" />
            </div>
          </Card>
        )}
      </div>
    </div>
  );
}
