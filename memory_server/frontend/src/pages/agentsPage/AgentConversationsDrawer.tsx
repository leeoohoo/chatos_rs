import {
  Card,
  Drawer,
  Empty,
  List,
  Space,
  Spin,
  Tag,
  Typography,
} from 'antd';

import type {
  AgentConversationPanelState,
  AgentPageTranslate,
} from './types';

const { Text } = Typography;

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
            style={{ flex: 1, minWidth: 0, height: '100%' }}
            styles={{ body: { height: '100%', overflowY: 'auto' } }}
          >
            {state.messagesLoading ? (
              <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 24 }}>
                <Spin />
              </div>
            ) : state.messages.length === 0 ? (
              <Empty description={t('agents.noConversations')} />
            ) : (
              <List
                size="small"
                dataSource={state.messages}
                renderItem={(message) => (
                  <List.Item>
                    <Space direction="vertical" size={2} style={{ width: '100%' }}>
                      <Space size={8}>
                        <Tag>{message.role}</Tag>
                        <Text type="secondary" style={{ fontSize: 12 }}>
                          {new Date(message.created_at).toLocaleString()}
                        </Text>
                      </Space>
                      <Text style={{ whiteSpace: 'pre-wrap' }}>
                        {message.content || '-'}
                      </Text>
                    </Space>
                  </List.Item>
                )}
              />
            )}
          </Card>
        </div>
      )}
    </Drawer>
  );
}
