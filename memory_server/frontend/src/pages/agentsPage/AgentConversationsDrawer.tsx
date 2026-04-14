import {
  Button,
  Card,
  Drawer,
  Empty,
  List,
  Pagination,
  Space,
  Spin,
  Tag,
  Typography,
} from 'antd';
import { useEffect, useMemo, useState } from 'react';

import type {
  AgentConversationPanelState,
  AgentPageTranslate,
} from './types';

const { Text } = Typography;
const MESSAGE_PAGE_SIZE_OPTIONS = ['10', '20', '50'];

interface AgentConversationsDrawerProps {
  t: AgentPageTranslate;
  state: AgentConversationPanelState;
  onClose: () => void;
  onSelectSession: (sessionId: string) => void | Promise<void>;
}

export function AgentConversationsDrawer({
  t,
  state,
  onClose,
  onSelectSession,
}: AgentConversationsDrawerProps) {
  const [messagePage, setMessagePage] = useState(1);
  const [messagePageSize, setMessagePageSize] = useState(20);
  const [expandedToolMessageIds, setExpandedToolMessageIds] = useState<Record<string, boolean>>({});

  useEffect(() => {
    setMessagePage(1);
    setExpandedToolMessageIds({});
  }, [state.sessionId, state.open]);

  const totalMessagePages = useMemo(
    () => Math.max(1, Math.ceil(state.messages.length / messagePageSize)),
    [state.messages.length, messagePageSize],
  );

  useEffect(() => {
    if (messagePage > totalMessagePages) {
      setMessagePage(totalMessagePages);
    }
  }, [messagePage, totalMessagePages]);

  const pagedMessages = useMemo(() => {
    const start = (messagePage - 1) * messagePageSize;
    return state.messages.slice(start, start + messagePageSize);
  }, [state.messages, messagePage, messagePageSize]);

  const toggleToolMessageExpanded = (messageId: string) => {
    setExpandedToolMessageIds((prev) => ({
      ...prev,
      [messageId]: !prev[messageId],
    }));
  };

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
                  <Text strong style={{ color: '#0958d9', fontSize: 13 }}>
                    {group.projectName}
                  </Text>
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
            styles={{
              body: {
                flex: 1,
                display: 'flex',
                flexDirection: 'column',
                minHeight: 0,
                overflow: 'hidden',
              },
            }}
          >
            {state.messagesLoading ? (
              <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 24 }}>
                <Spin />
              </div>
            ) : state.messages.length === 0 ? (
              <Empty description={t('agents.noConversations')} />
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
                <div style={{ flex: 1, minHeight: 0, overflowY: 'auto' }}>
                  <List
                    size="small"
                    dataSource={pagedMessages}
                    renderItem={(message) => {
                      const role = (message.role || '').trim().toLowerCase();
                      const isToolMessage = role === 'tool';
                      const expanded = Boolean(expandedToolMessageIds[message.id]);
                      const rawContent = message.content || '-';

                      return (
                        <List.Item>
                          <Space direction="vertical" size={2} style={{ width: '100%' }}>
                            <Space size={8}>
                              <Tag>{message.role}</Tag>
                              <Text type="secondary" style={{ fontSize: 12 }}>
                                {new Date(message.created_at).toLocaleString()}
                              </Text>
                            </Space>
                            {isToolMessage ? (
                              <Space direction="vertical" size={6} style={{ width: '100%' }}>
                                {expanded ? (
                                  <Text type="secondary" style={{ whiteSpace: 'pre-wrap' }}>
                                    {rawContent}
                                  </Text>
                                ) : (
                                  <Text type="secondary">工具调用内容已折叠</Text>
                                )}
                                <Button
                                  type="link"
                                  size="small"
                                  style={{ paddingInline: 0, width: 'fit-content' }}
                                  onClick={() => toggleToolMessageExpanded(message.id)}
                                >
                                  {expanded ? '收起工具调用内容' : '展开工具调用内容'}
                                </Button>
                              </Space>
                            ) : (
                              <Text style={{ whiteSpace: 'pre-wrap' }}>
                                {rawContent}
                              </Text>
                            )}
                          </Space>
                        </List.Item>
                      );
                    }}
                  />
                </div>
                <div style={{ flexShrink: 0, borderTop: '1px solid #f0f0f0', paddingTop: 10, marginTop: 8 }}>
                  <Pagination
                    size="small"
                    current={messagePage}
                    pageSize={messagePageSize}
                    total={state.messages.length}
                    showSizeChanger
                    pageSizeOptions={MESSAGE_PAGE_SIZE_OPTIONS}
                    onChange={(page, pageSize) => {
                      setMessagePage(page);
                      if (pageSize !== messagePageSize) {
                        setMessagePageSize(pageSize);
                      }
                    }}
                    showTotal={(total, range) => `${range[0]}-${range[1]} / ${total}`}
                  />
                </div>
              </div>
            )}
          </Card>
        </div>
      )}
    </Drawer>
  );
}
