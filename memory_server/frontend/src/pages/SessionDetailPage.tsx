import { useEffect, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Empty,
  Input,
  Select,
  Space,
  Table,
  Tabs,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { Message, SessionSummary } from '../types';

interface SessionDetailPageProps {
  sessionId?: string;
}

export function SessionDetailPage({ sessionId }: SessionDetailPageProps) {
  const { t } = useI18n();
  const [messages, setMessages] = useState<Message[]>([]);
  const [summaries, setSummaries] = useState<SessionSummary[]>([]);
  const [contextPreview, setContextPreview] = useState<string>('');
  const [newRole, setNewRole] = useState('user');
  const [newMessage, setNewMessage] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    if (!sessionId) {
      setMessages([]);
      setSummaries([]);
      setContextPreview('');
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const [msg, sum, ctx] = await Promise.all([
        api.listMessages(sessionId),
        api.listSummaries(sessionId),
        api.composeContext(sessionId),
      ]);
      setMessages(msg);
      setSummaries(sum);
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

  const handleAddMessage = async () => {
    if (!sessionId || !newMessage.trim()) {
      return;
    }

    try {
      await api.createMessage(sessionId, { role: newRole, content: newMessage.trim() });
      setNewMessage('');
      await load();
    } catch (err) {
      setError((err as Error).message);
    }
  };

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

  return (
    <Card
      title={t('sessionDetail.title')}
      extra={
        <Button onClick={load} loading={loading}>
          {t('common.refresh')}
        </Button>
      }
    >
      <Typography.Paragraph type="secondary" style={{ marginBottom: 12 }}>
        {t('sessionDetail.sessionLabel')}: {sessionId}
      </Typography.Paragraph>

      <Space wrap style={{ marginBottom: 12 }}>
        <Select
          value={newRole}
          onChange={setNewRole}
          style={{ width: 140 }}
          options={[
            { label: 'user', value: 'user' },
            { label: 'assistant', value: 'assistant' },
            { label: 'tool', value: 'tool' },
            { label: 'system', value: 'system' },
          ]}
        />
        <Input
          value={newMessage}
          onChange={(e) => setNewMessage(e.target.value)}
          placeholder={t('sessionDetail.addMessage')}
          style={{ width: 520 }}
        />
        <Button type="primary" onClick={handleAddMessage}>
          {t('sessionDetail.add')}
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
        ]}
      />
    </Card>
  );
}
