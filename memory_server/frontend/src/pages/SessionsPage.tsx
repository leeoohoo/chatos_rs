import { useEffect, useState } from 'react';
import { Alert, Button, Card, Input, Space, Table, Tag } from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { Session } from '../types';
import { SessionDetailPage } from './SessionDetailPage';

interface SessionsPageProps {
  filterUserId?: string;
  currentUserId: string;
  isAdmin: boolean;
  selectedSessionId?: string;
  onSelectSession: (sessionId: string, sessionUserId: string) => void;
}

export function SessionsPage({
  filterUserId,
  currentUserId,
  isAdmin,
  selectedSessionId,
  onSelectSession,
}: SessionsPageProps) {
  const { t } = useI18n();
  const [sessions, setSessions] = useState<Session[]>([]);
  const [title, setTitle] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detailSessionId, setDetailSessionId] = useState<string | undefined>(undefined);
  const normalizedFilterUserId = filterUserId?.trim() || undefined;

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const scopeUserId = isAdmin ? normalizedFilterUserId : currentUserId.trim();
      if (!scopeUserId && !isAdmin) {
        setSessions([]);
        return;
      }
      const items = await api.listSessions(scopeUserId);
      setSessions(items);
      if (!selectedSessionId && items.length > 0) {
        onSelectSession(items[0].id, items[0].user_id);
      }
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filterUserId, currentUserId, isAdmin]);

  const handleCreate = async () => {
    const createUserId = isAdmin ? normalizedFilterUserId || currentUserId : currentUserId;
    if (!createUserId.trim()) {
      setError(t('sessions.needUserId'));
      return;
    }

    setError(null);
    try {
      const created = await api.createSession(createUserId, title.trim() || undefined);
      setTitle('');
      await load();
      onSelectSession(created.id, created.user_id);
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const columns: ColumnsType<Session> = [
    {
      title: t('sessions.id'),
      dataIndex: 'id',
      key: 'id',
      width: 120,
      render: (value: string) => value.slice(0, 8),
    },
    {
      title: t('sessions.titleCol'),
      dataIndex: 'title',
      key: 'title',
      ellipsis: true,
      render: (value?: string | null) => value || '-',
    },
    {
      title: t('sessions.user'),
      dataIndex: 'user_id',
      key: 'user_id',
      width: 180,
      render: (value: string) => value || '-',
    },
    {
      title: t('sessions.status'),
      dataIndex: 'status',
      key: 'status',
      width: 120,
      render: (value: string) => <Tag color="default">{value}</Tag>,
    },
    {
      title: t('sessions.updatedAt'),
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 220,
      render: (value: string) => new Date(value).toLocaleString(),
    },
  ];

  if (detailSessionId) {
    return (
      <SessionDetailPage
        sessionId={detailSessionId}
        onBack={() => setDetailSessionId(undefined)}
      />
    );
  }

  return (
    <Card
      title={t('sessions.title')}
      extra={
        <Button onClick={load} loading={loading}>
          {t('common.refresh')}
        </Button>
      }
    >
      <Space wrap style={{ marginBottom: 12 }}>
        <Input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={t('sessions.newTitle')}
          style={{ width: 300 }}
        />
        <Button type="primary" onClick={handleCreate}>
          {t('sessions.create')}
        </Button>
      </Space>

      {isAdmin && !normalizedFilterUserId && (
        <Alert type="info" showIcon message={t('sessions.adminAllTip')} style={{ marginBottom: 12 }} />
      )}
      {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}

      <Table
        rowKey="id"
        loading={loading}
        columns={columns}
        dataSource={sessions}
        pagination={false}
        size="middle"
        rowClassName={(record) =>
          record.id === selectedSessionId ? 'memory-row-selected' : ''
        }
        onRow={(record) => ({
          onClick: () => {
            onSelectSession(record.id, record.user_id);
            setDetailSessionId(record.id);
          },
          style: { cursor: 'pointer' },
        })}
      />
    </Card>
  );
}
