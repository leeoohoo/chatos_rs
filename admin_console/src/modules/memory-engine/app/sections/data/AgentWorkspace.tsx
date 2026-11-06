// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Card, Empty, Space, Statistic, Table, Tabs, Tag, Typography } from 'antd';
import type { TableColumnsType, TabsProps } from 'antd';
import { useEffect, useState } from 'react';

import type {
  EngineAgentConversation,
  EngineAgentDirectoryItem,
  EngineAgentProject,
  EngineSubjectMemory,
} from '../../../types';
import { formatStructuredText, statusColor, toLocal } from '../../utils';
import { ThreadDetailWorkspace } from './ThreadWorkspace';
import type { AgentWorkspaceProps, UserLabelMap } from './types';

const { Paragraph, Text, Title } = Typography;

function displayName(value: string, explicit?: string | null): string {
  return explicit?.trim() || value;
}

function tenantDisplayText(tenantId: string, tenantLabelsById: UserLabelMap): string {
  const user = tenantLabelsById[tenantId];
  if (!user) return tenantId;
  const username = user.username.trim();
  const name = user.display_name.trim() || username || tenantId;
  return username && username !== name ? `${name} (${username})` : name;
}

function memoryColumns(): TableColumnsType<EngineSubjectMemory> {
  return [
    {
      title: '类型',
      dataIndex: 'memory_type',
      key: 'memory_type',
      width: 140,
      render: (value: string) => <Tag>{value}</Tag>,
    },
    {
      title: '层级',
      dataIndex: 'level',
      key: 'level',
      width: 70,
      align: 'right',
    },
    {
      title: '记忆内容',
      dataIndex: 'text',
      key: 'text',
      render: (value?: string | null) => (
        <Paragraph ellipsis={{ rows: 4 }} style={{ marginBottom: 0 }}>
          {value?.trim() || '-'}
        </Paragraph>
      ),
    },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 100,
      render: (value: string) => <Tag color={statusColor(value)}>{value}</Tag>,
    },
    {
      title: '更新',
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 180,
      render: toLocal,
    },
  ];
}

function MemoryTable(props: {
  items: EngineSubjectMemory[];
  loading: boolean;
  emptyText: string;
}) {
  return (
    <Table<EngineSubjectMemory>
      className="engine-fill-table"
      rowKey="id"
      dataSource={props.items}
      loading={props.loading}
      columns={memoryColumns()}
      pagination={{ pageSize: 10 }}
      scroll={{ x: 900, y: 'calc(100vh - 470px)' }}
      locale={{ emptyText: props.emptyText }}
      expandable={{
        expandedRowRender: (memory) => (
          <pre className="engine-record-modal-pre">{formatStructuredText(memory)}</pre>
        ),
      }}
    />
  );
}

function agentColumns(
  selected: EngineAgentDirectoryItem | null,
  labels: UserLabelMap,
): TableColumnsType<EngineAgentDirectoryItem> {
  return [
    {
      title: 'Agent',
      key: 'agent',
      render: (_value, agent) => (
        <div className="engine-thread-cell">
          <Text strong>{displayName(agent.agent_id, agent.display_name)}</Text>
          <Text type="secondary" copyable>{agent.agent_id}</Text>
        </div>
      ),
    },
    {
      title: '用户 / 来源',
      key: 'scope',
      width: 210,
      render: (_value, agent) => (
        <Space direction="vertical" size={2}>
          <Text>{tenantDisplayText(agent.tenant_id, labels)}</Text>
          <Text type="secondary">{agent.source_id}</Text>
        </Space>
      ),
    },
    {
      title: '记忆 / 项目 / 原始线程',
      key: 'counts',
      width: 160,
      render: (_value, agent) => (
        <Text type="secondary">
          {agent.memory_count} / {agent.project_count} / {agent.thread_count}
        </Text>
      ),
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
      render: (_value, agent) =>
        agent.agent_subject_id === selected?.agent_subject_id &&
        agent.tenant_id === selected.tenant_id &&
        agent.source_id === selected.source_id
          ? <span aria-label="selected" />
          : null,
    },
  ];
}

function projectColumns(): TableColumnsType<EngineAgentProject> {
  return [
    {
      title: '项目',
      key: 'project',
      render: (_value, project) => (
        <div className="engine-thread-cell">
          <Text strong>{displayName(project.project_id, project.display_name)}</Text>
          <Text type="secondary" copyable>{project.project_id}</Text>
        </div>
      ),
    },
    { title: '原始线程', dataIndex: 'thread_count', key: 'thread_count', width: 100 },
    { title: '总结', dataIndex: 'memory_count', key: 'memory_count', width: 90 },
    {
      title: '更新',
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 180,
      render: toLocal,
    },
  ];
}

function conversationProjectName(conversation: EngineAgentConversation): string {
  if (!conversation.project_subject_id) return 'Main Chat（无项目）';
  return displayName(
    conversation.project_subject_id.replace(/^project:/, ''),
    conversation.project_display_name,
  );
}

function conversationKindLabel(kind: string): string {
  switch (kind) {
    case 'agent_task':
      return 'Task';
    case 'todo_execution':
      return 'Todo';
    case 'main_chat':
      return 'Main Chat';
    case 'agent_conversation':
      return 'Agent 会话';
    case 'approval_agent':
      return '审批会话';
    case 'agent_team_run':
      return 'Agent 团队任务';
    default:
      return kind;
  }
}

function conversationIdentity(conversation: EngineAgentConversation): string {
  const conversationId = conversation.conversation_id?.trim();
  if (conversationId) return conversationId;
  if (conversation.thread_kind === 'main_chat' || conversation.thread_kind === 'agent_conversation') {
    return '历史记录（无会话 ID）';
  }
  return conversation.thread_id;
}

function conversationColumns(): TableColumnsType<EngineAgentConversation> {
  return [
    {
      title: '项目 / 会话',
      key: 'conversation',
      render: (_value, conversation) => (
        <div className="engine-thread-cell">
          <Text strong>{conversationProjectName(conversation)}</Text>
          <Text type="secondary" copyable>
            {conversationIdentity(conversation)}
          </Text>
        </div>
      ),
    },
    {
      title: '类型',
      dataIndex: 'thread_kind',
      key: 'thread_kind',
      width: 110,
      render: (value: string) => <Tag>{conversationKindLabel(value)}</Tag>,
    },
    {
      title: '记录',
      dataIndex: 'record_count',
      key: 'record_count',
      width: 80,
      align: 'right',
    },
    {
      title: '状态',
      key: 'status',
      width: 120,
      render: (_value, conversation) => (
        <Space direction="vertical" size={2}>
          <Tag color={statusColor(conversation.status)}>{conversation.status}</Tag>
          <Tag>{conversation.summary_status}</Tag>
        </Space>
      ),
    },
    {
      title: '更新',
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 180,
      render: toLocal,
    },
  ];
}

export function AgentWorkspace(props: AgentWorkspaceProps) {
  const [activeTab, setActiveTab] = useState('memories');

  useEffect(() => {
    setActiveTab('memories');
  }, [props.selectedAgent?.agent_subject_id, props.selectedAgent?.tenant_id, props.selectedAgent?.source_id]);

  const agent = props.selectedAgent;
  const detailItems: TabsProps['items'] = agent
    ? [
        {
          key: 'memories',
          label: `Agent 记忆 (${props.agentMemories.length})`,
          children: (
            <div className="engine-data-tab-pane">
              <MemoryTable
                items={props.agentMemories}
                loading={props.agentDetailLoading}
                emptyText="该 Agent 尚未形成长期记忆"
              />
            </div>
          ),
        },
        {
          key: 'projects',
          label: `项目 (${props.projects.length})`,
          children: (
            <div className="engine-agent-project-pane">
              <Table<EngineAgentProject>
                rowKey="project_subject_id"
                dataSource={props.projects}
                loading={props.agentDetailLoading}
                columns={projectColumns()}
                pagination={false}
                scroll={{ x: 700, y: 220 }}
                rowClassName={(project) =>
                  project.project_subject_id === props.selectedProject?.project_subject_id
                    ? 'engine-thread-row engine-thread-row--selected'
                    : 'engine-thread-row'
                }
                onRow={(project) => ({ onClick: () => props.onSelectProject(project) })}
              />
              <div className="engine-agent-project-memory">
                <Title level={5}>
                  {props.selectedProject
                    ? `${displayName(props.selectedProject.project_id, props.selectedProject.display_name)} · 项目总结`
                    : '项目总结'}
                </Title>
                <MemoryTable
                  items={props.projectMemories}
                  loading={props.projectMemoriesLoading}
                  emptyText={props.selectedProject ? '该项目尚未形成项目总结' : '选择一个项目'}
                />
              </div>
            </div>
          ),
        },
        {
          key: 'threads',
          label: `会话 (${props.conversations.length})`,
          children: (
            <div className="engine-agent-thread-pane">
              <div className="engine-agent-conversation-workspace">
                <Card className="engine-data-card" title="按项目划分的会话">
                  <Table<EngineAgentConversation>
                    className="engine-fill-table"
                    rowKey={(conversation) => [
                      conversation.thread_id,
                      conversation.project_subject_id ?? '',
                      conversation.conversation_id ?? '',
                    ].join('::')}
                    dataSource={props.conversations}
                    loading={props.agentDetailLoading}
                    columns={conversationColumns()}
                    pagination={{ pageSize: 20, showSizeChanger: true }}
                    scroll={{ x: 820, y: 'calc(100vh - 470px)' }}
                    rowClassName={(conversation) =>
                      conversation.thread_id === props.selectedConversation?.thread_id &&
                      conversation.project_subject_id === props.selectedConversation?.project_subject_id &&
                      conversation.conversation_id === props.selectedConversation?.conversation_id
                        ? 'engine-thread-row engine-thread-row--selected'
                        : 'engine-thread-row'
                    }
                    onRow={(conversation) => ({
                      onClick: () => props.onSelectConversation(conversation),
                    })}
                    locale={{ emptyText: '该 Agent 尚无会话记录' }}
                  />
                </Card>
                <ThreadDetailWorkspace
                  {...props}
                  showMemories={false}
                  title={props.selectedConversation
                    ? `${conversationProjectName(props.selectedConversation)} · ${conversationKindLabel(props.selectedConversation.thread_kind)}`
                    : undefined}
                  extra={props.selectedConversation?.conversation_id ?? props.selectedConversation?.thread_id}
                  emptyText="选择一个项目会话查看记录和总结"
                />
              </div>
            </div>
          ),
        },
      ]
    : [];

  return (
    <div className="engine-agent-workspace">
      <Card className="engine-data-card" title={`Agent（${props.agents.length}）`}>
        <Table<EngineAgentDirectoryItem>
          className="engine-fill-table"
          rowKey={(item) => `${item.tenant_id}::${item.source_id}::${item.agent_subject_id}`}
          dataSource={props.agents}
          loading={props.agentsLoading}
          columns={agentColumns(agent, props.tenantLabelsById ?? {})}
          pagination={{ pageSize: 20, showSizeChanger: true }}
          scroll={{ x: 760, y: 'calc(100vh - 390px)' }}
          rowClassName={(item) =>
            item.agent_subject_id === agent?.agent_subject_id &&
            item.tenant_id === agent.tenant_id &&
            item.source_id === agent.source_id
              ? 'engine-thread-row engine-thread-row--selected'
              : 'engine-thread-row'
          }
          onRow={(item) => ({ onClick: () => props.onSelectAgent(item) })}
        />
      </Card>
      {agent ? (
        <Card
          className="engine-data-card engine-agent-detail-card"
          title={displayName(agent.agent_id, agent.display_name)}
          extra={<Text type="secondary" copyable>{agent.agent_subject_id}</Text>}
        >
          <div className="engine-agent-statistics">
            <Statistic title="长期记忆" value={agent.memory_count} />
            <Statistic title="参与项目" value={agent.project_count} />
            <Statistic title="项目会话" value={props.conversations.length} />
            <Statistic title="最近活动" value={toLocal(agent.updated_at)} />
          </div>
          <Tabs
            className="engine-data-detail-tabs engine-agent-detail-tabs"
            activeKey={activeTab}
            onChange={setActiveTab}
            items={detailItems}
          />
        </Card>
      ) : (
        <Card className="engine-data-card engine-data-card--empty">
          <Empty description="没有符合条件的 Agent" />
        </Card>
      )}
    </div>
  );
}
