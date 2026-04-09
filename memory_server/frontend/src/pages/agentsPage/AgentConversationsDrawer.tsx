import {
  Button,
  Card,
  Collapse,
  Drawer,
  Empty,
  List,
  Pagination,
  Popconfirm,
  Space,
  Spin,
  Tag,
  Typography,
} from 'antd';

import type {
  AgentConversationPanelState,
  AgentPageTranslate,
} from './types';

const { Text, Paragraph } = Typography;

function stringifyPretty(value: unknown): string {
  if (value == null) {
    return '';
  }
  if (typeof value === 'string') {
    return value;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function truncateText(value?: string | null, maxLength = 180): string {
  const normalized = (value || '').trim();
  if (!normalized) {
    return '-';
  }
  if (normalized.length <= maxLength) {
    return normalized;
  }
  return `${normalized.slice(0, maxLength)}...`;
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function extractToolEntries(value: unknown): Array<Record<string, unknown>> {
  if (Array.isArray(value)) {
    return value.filter((item): item is Record<string, unknown> => isObjectRecord(item));
  }
  if (isObjectRecord(value)) {
    return [value];
  }
  return [];
}

function getToolEntryLabel(entry: Record<string, unknown>, index: number): string {
  const directName = typeof entry.name === 'string' ? entry.name : null;
  const directType = typeof entry.type === 'string' ? entry.type : null;
  const fnBlock = isObjectRecord(entry.function) ? entry.function : null;
  const fnName = fnBlock && typeof fnBlock.name === 'string' ? fnBlock.name : null;
  return fnName || directName || directType || `工具调用 ${index + 1}`;
}

interface AgentConversationsDrawerProps {
  t: AgentPageTranslate;
  state: AgentConversationPanelState;
  onClose: () => void;
  onSelectSession: (sessionId: string, page?: number) => void | Promise<void>;
  onClearMessages: (sessionId: string) => void | Promise<void>;
}

export function AgentConversationsDrawer({
  t,
  state,
  onClose,
  onSelectSession,
  onClearMessages,
}: AgentConversationsDrawerProps) {
  const messagesTotalEstimate = state.messagesHasMore
    ? state.messagesPage * state.messagesPageSize + 1
    : (state.messagesPage - 1) * state.messagesPageSize + state.messages.length;

  return (
    <Drawer
      open={state.open}
      onClose={onClose}
      width={980}
      styles={{ body: { paddingTop: 12, overflow: 'hidden' } }}
      title={`${t('agents.conversationsTitle')}: ${state.agent?.name || '-'}`}
    >
      {state.loading ? (
        <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 48 }}>
          <Spin />
        </div>
      ) : state.sessions.length === 0 ? (
        <Empty description={t('agents.noConversations')} />
      ) : (
        <div
          style={{
            display: 'flex',
            gap: 12,
            width: '100%',
            height: 'calc(100vh - 210px)',
            minHeight: 420,
            overflow: 'hidden',
          }}
        >
          <Card
            title={t('agents.projectSessions')}
            style={{ width: 320, height: '100%', flexShrink: 0 }}
            styles={{ body: { height: '100%', overflowY: 'auto' } }}
          >
            <Space direction="vertical" size={10} style={{ width: '100%' }}>
              {state.groupedSessions.map((group) => (
                <div key={group.projectId}>
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      gap: 8,
                      marginBottom: 6,
                    }}
                  >
                    <Text strong style={{ color: '#0958d9', fontSize: 13 }}>
                      {group.projectName}
                    </Text>
                    <Popconfirm
                      title={t('agents.clearHistoryConfirm')}
                      okText={t('common.confirm')}
                      cancelText={t('common.cancel')}
                      onConfirm={() => {
                        void onClearMessages(group.session.id);
                      }}
                      disabled={state.clearing}
                    >
                      <Button size="small" danger loading={state.clearing} disabled={state.clearing}>
                        {t('agents.clearHistory')}
                      </Button>
                    </Popconfirm>
                  </div>
                  <List
                    size="small"
                    dataSource={[group.session]}
                    renderItem={(session) => {
                      const active = state.sessionId === session.id;
                      return (
                        <List.Item
                          style={{
                            cursor: 'pointer',
                            background: active ? '#f0f5ff' : undefined,
                            borderRadius: 6,
                            paddingInline: 8,
                          }}
                          onClick={() => {
                            void onSelectSession(session.id);
                          }}
                        >
                          <Space direction="vertical" size={0} style={{ width: '100%' }}>
                            <Text strong>{session.title || t('agents.untitledSession')}</Text>
                            <Text type="secondary" style={{ fontSize: 12 }}>
                              {new Date(session.updated_at).toLocaleString()}
                            </Text>
                          </Space>
                        </List.Item>
                      );
                    }}
                  />
                </div>
              ))}
            </Space>
          </Card>

          <Card
            title={t('agents.messages')}
            style={{ flex: 1, minWidth: 0, height: '100%', display: 'flex', flexDirection: 'column' }}
            styles={{ body: { flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', overflow: 'hidden' } }}
          >
            {state.messagesLoading ? (
              <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 24 }}>
                <Spin />
              </div>
            ) : state.messages.length === 0 ? (
              <Empty description={t('agents.noConversations')} />
            ) : (
              <>
                <div style={{ flex: 1, minHeight: 0, overflowY: 'auto', paddingRight: 4 }}>
                  <List
                    size="small"
                    dataSource={state.messages}
                    renderItem={(message) => {
                      const toolEntries = extractToolEntries(message.tool_calls);
                      const hasToolDetails =
                        message.role === 'tool'
                        || toolEntries.length > 0
                        || Boolean(message.tool_call_id);

                      return (
                        <List.Item style={{ paddingInline: 0 }}>
                          <Card size="small" bodyStyle={{ padding: 12 }} style={{ width: '100%' }}>
                            <Space direction="vertical" size={6} style={{ width: '100%' }}>
                              <Space size={8} wrap>
                                <Tag color={message.role === 'assistant' ? 'blue' : message.role === 'tool' ? 'purple' : 'default'}>
                                  {message.role}
                                </Tag>
                                {message.message_source && <Tag>{message.message_source}</Tag>}
                                {message.summary_status && <Tag>{message.summary_status}</Tag>}
                                <Text type="secondary" style={{ fontSize: 12 }}>
                                  {new Date(message.created_at).toLocaleString()}
                                </Text>
                              </Space>
                              {hasToolDetails ? (
                                <>
                                  <Text type="secondary">
                                    {truncateText(message.content, 180)}
                                  </Text>
                                  <Collapse
                                    ghost
                                    items={[
                                      {
                                        key: 'tool-details',
                                        label: (
                                          <Space wrap>
                                            <Text>工具调用详情</Text>
                                            {toolEntries.length > 0 && <Tag>{toolEntries.length} 个</Tag>}
                                          </Space>
                                        ),
                                        children: (
                                          <Space direction="vertical" size={8} style={{ width: '100%' }}>
                                            <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                              {message.content || '-'}
                                            </Paragraph>
                                            {message.tool_call_id && (
                                              <Text type="secondary">
                                                tool_call_id:
                                                {' '}
                                                {message.tool_call_id}
                                              </Text>
                                            )}
                                            {toolEntries.length > 0 && (
                                              <Space direction="vertical" size={8} style={{ width: '100%' }}>
                                                {toolEntries.map((entry, index) => (
                                                  <Card
                                                    key={`${message.id}-tool-${index}`}
                                                    size="small"
                                                    bodyStyle={{ padding: 10 }}
                                                  >
                                                    <Space direction="vertical" size={6} style={{ width: '100%' }}>
                                                      <Text strong>{getToolEntryLabel(entry, index)}</Text>
                                                      <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                                                        {stringifyPretty(entry)}
                                                      </pre>
                                                    </Space>
                                                  </Card>
                                                ))}
                                              </Space>
                                            )}
                                            {message.metadata && (
                                              <Card size="small" bodyStyle={{ padding: 10 }}>
                                                <Space direction="vertical" size={4} style={{ width: '100%' }}>
                                                  <Text strong>metadata</Text>
                                                  <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                                                    {stringifyPretty(message.metadata)}
                                                  </pre>
                                                </Space>
                                              </Card>
                                            )}
                                            {message.reasoning && (
                                              <Paragraph type="secondary" style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                                {message.reasoning}
                                              </Paragraph>
                                            )}
                                          </Space>
                                        ),
                                      },
                                    ]}
                                  />
                                </>
                              ) : (
                                <>
                                  <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                    {message.content || '-'}
                                  </Paragraph>
                                  {message.reasoning && (
                                    <Paragraph type="secondary" style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                      {message.reasoning}
                                    </Paragraph>
                                  )}
                                  {message.metadata && (
                                    <Collapse
                                      ghost
                                      items={[
                                        {
                                          key: 'message-metadata',
                                          label: '附加信息',
                                          children: (
                                            <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                                              {stringifyPretty(message.metadata)}
                                            </pre>
                                          ),
                                        },
                                      ]}
                                    />
                                  )}
                                </>
                              )}
                            </Space>
                          </Card>
                        </List.Item>
                      );
                    }}
                  />
                </div>
                <div
                  style={{
                    flexShrink: 0,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    gap: 12,
                    marginTop: 12,
                    paddingTop: 12,
                    borderTop: '1px solid #f0f0f0',
                    background: '#fff',
                  }}
                >
                  <Text type="secondary" style={{ fontSize: 12 }}>
                    第 {state.messagesPage} 页
                  </Text>
                  <Pagination
                    size="small"
                    current={state.messagesPage}
                    pageSize={state.messagesPageSize}
                    total={messagesTotalEstimate}
                    showSizeChanger={false}
                    showLessItems
                    onChange={(page) => {
                      if (!state.sessionId) {
                        return;
                      }
                      void onSelectSession(state.sessionId, page);
                    }}
                  />
                </div>
              </>
            )}
          </Card>
        </div>
      )}
    </Drawer>
  );
}
