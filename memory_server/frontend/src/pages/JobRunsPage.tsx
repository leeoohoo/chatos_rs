import { useEffect, useState } from 'react';
import { Alert, Button, Card, Table } from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { JobRun } from '../types';

export function JobRunsPage() {
  const { t } = useI18n();
  const [items, setItems] = useState<JobRun[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.listJobRuns();
      setItems(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
  }, []);

  const columns: ColumnsType<JobRun> = [
    { title: t('jobRuns.jobType'), dataIndex: 'job_type', key: 'job_type', width: 160 },
    {
      title: t('jobRuns.session'),
      dataIndex: 'session_id',
      key: 'session_id',
      width: 120,
      render: (value?: string | null) => (value ? value.slice(0, 8) : '-'),
    },
    { title: t('jobRuns.status'), dataIndex: 'status', key: 'status', width: 120 },
    { title: t('jobRuns.input'), dataIndex: 'input_count', key: 'input_count', width: 100 },
    { title: t('jobRuns.output'), dataIndex: 'output_count', key: 'output_count', width: 100 },
    {
      title: t('jobRuns.startedAt'),
      dataIndex: 'started_at',
      key: 'started_at',
      width: 200,
      render: (value: string) => new Date(value).toLocaleString(),
    },
    { title: t('jobRuns.error'), dataIndex: 'error_message', key: 'error_message', ellipsis: true },
  ];

  return (
    <Card
      title={t('jobRuns.title')}
      extra={
        <Button onClick={load} loading={loading}>
          {t('common.refresh')}
        </Button>
      }
    >
      {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
      <Table
        rowKey="id"
        loading={loading}
        columns={columns}
        dataSource={items}
        pagination={{ pageSize: 12 }}
        size="small"
      />
    </Card>
  );
}
