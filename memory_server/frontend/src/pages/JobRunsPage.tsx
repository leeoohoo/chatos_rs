import { useEffect, useMemo, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Col,
  Input,
  Row,
  Select,
  Space,
  Statistic,
  Switch,
  Table,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { JobRun } from '../types';

const { Text } = Typography;

function toLocal(value?: string | null): string {
  if (!value) {
    return '-';
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

function formatDuration(startedAt?: string, finishedAt?: string | null): string {
  if (!startedAt) {
    return '-';
  }
  const start = new Date(startedAt).getTime();
  if (Number.isNaN(start)) {
    return '-';
  }
  const end = finishedAt ? new Date(finishedAt).getTime() : Date.now();
  if (Number.isNaN(end) || end < start) {
    return '-';
  }
  const totalSeconds = Math.floor((end - start) / 1000);
  if (totalSeconds < 60) {
    return `${totalSeconds}s`;
  }
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}m ${seconds}s`;
}

function statusColor(status: string): string {
  if (status === 'done') {
    return 'success';
  }
  if (status === 'failed') {
    return 'error';
  }
  if (status === 'running') {
    return 'processing';
  }
  return 'default';
}

function formatCount(value?: number | null): string {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return '-';
  }
  return String(value);
}

export function JobRunsPage() {
  const { t } = useI18n();
  const [items, setItems] = useState<JobRun[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [jobType, setJobType] = useState<string>('all');
  const [status, setStatus] = useState<string>('all');
  const [sessionKeyword, setSessionKeyword] = useState<string>('');
  const [autoRefresh, setAutoRefresh] = useState<boolean>(true);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.listJobRuns({
        limit: 500,
        job_type: jobType === 'all' ? undefined : jobType,
        status: status === 'all' ? undefined : status,
      });
      setItems(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jobType, status]);

  useEffect(() => {
    if (!autoRefresh) {
      return;
    }
    const timer = window.setInterval(() => {
      void load();
    }, 10000);
    return () => window.clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoRefresh, jobType, status]);

  const visibleItems = useMemo(() => {
    const kw = sessionKeyword.trim().toLowerCase();
    if (!kw) {
      return items;
    }
    return items.filter((item) => {
      const rawId = (item.session_id || '').toLowerCase();
      const display = (item.session_display || '').toLowerCase();
      const projectLabel = (item.session_project_label || '').toLowerCase();
      const agentLabel = (item.session_agent_label || '').toLowerCase();
      return (
        rawId.includes(kw) ||
        display.includes(kw) ||
        projectLabel.includes(kw) ||
        agentLabel.includes(kw)
      );
    });
  }, [items, sessionKeyword]);

  const stats = useMemo(() => {
    let running = 0;
    let done = 0;
    let failed = 0;
    for (const row of visibleItems) {
      if (row.status === 'running') {
        running += 1;
      } else if (row.status === 'done') {
        done += 1;
      } else if (row.status === 'failed') {
        failed += 1;
      }
    }
    return {
      total: visibleItems.length,
      running,
      done,
      failed,
    };
  }, [visibleItems]);

  const jobTypeOptions = useMemo(() => {
    const set = new Set(items.map((item) => item.job_type).filter(Boolean));
    return [
      { label: t('jobRuns.jobTypeAll'), value: 'all' },
      ...Array.from(set).sort().map((value) => ({ label: value, value })),
    ];
  }, [items, t]);

  const columns: ColumnsType<JobRun> = [
    {
      title: t('jobRuns.runId'),
      dataIndex: 'id',
      key: 'id',
      width: 110,
      render: (value: string) => (
        <Text code copyable={{ text: value }}>
          {value.slice(0, 8)}
        </Text>
      ),
    },
    {
      title: t('jobRuns.jobType'),
      dataIndex: 'job_type',
      key: 'job_type',
      width: 150,
      render: (value: string) => <Tag>{value}</Tag>,
    },
    {
      title: t('jobRuns.session'),
      dataIndex: 'session_id',
      key: 'session_id',
      width: 320,
      render: (value: string | null | undefined, row) =>
        value ? (
          <Space direction="vertical" size={0}>
            {row.session_resolve_status === 'found' ? (
              <>
                <Text>
                  {t('jobRuns.sessionContact')}: {row.session_contact_label || '-'}
                </Text>
                <Text>
                  {t('jobRuns.sessionProject')}: {row.session_project_label || t('jobRuns.projectUnassigned')}
                </Text>
                <Text>
                  {t('jobRuns.sessionAgent')}: {row.session_agent_label || '-'}
                </Text>
                {row.session_resolve_match_mode && row.session_resolve_match_mode !== 'exact' && (
                  <Tag color="gold">
                    {t('jobRuns.sessionMatchMode')}: {row.session_resolve_match_mode}
                  </Tag>
                )}
                {row.session_id_effective && row.session_id_effective !== value && (
                  <Text type="secondary">
                    {t('jobRuns.sessionEffectiveId')}: {row.session_id_effective.slice(0, 8)}
                  </Text>
                )}
                {(row.session_id_raw_len !== undefined || row.session_id_trimmed_len !== undefined) && (
                  <Text type="secondary">
                    {t('jobRuns.sessionIdLen')}: {row.session_id_raw_len ?? '-'} / {row.session_id_trimmed_len ?? '-'}
                  </Text>
                )}
                {row.session_resolve_detail && <Text type="secondary">{row.session_resolve_detail}</Text>}
              </>
            ) : (
              <>
                <Tag color="warning">
                  {row.session_resolve_status === 'missing_session'
                    ? t('jobRuns.sessionMissing')
                    : t('jobRuns.sessionLookupError')}
                </Tag>
                {row.session_resolve_match_mode && (
                  <Text type="secondary">
                    {t('jobRuns.sessionMatchMode')}: {row.session_resolve_match_mode}
                  </Text>
                )}
                {(row.session_id_raw_len !== undefined || row.session_id_trimmed_len !== undefined) && (
                  <Text type="secondary">
                    {t('jobRuns.sessionIdLen')}: {row.session_id_raw_len ?? '-'} / {row.session_id_trimmed_len ?? '-'}
                  </Text>
                )}
                {row.session_resolve_detail && (
                  <Text type="secondary">{row.session_resolve_detail}</Text>
                )}
              </>
            )}
            <Text code copyable={{ text: value }}>{t('jobRuns.sessionIdShort')}: {value.slice(0, 8)}</Text>
          </Space>
        ) : (
          '-'
        ),
    },
    {
      title: t('jobRuns.trigger'),
      dataIndex: 'trigger_type',
      key: 'trigger_type',
      width: 180,
      render: (value?: string | null) => (value && value.trim().length > 0 ? value : '-'),
    },
    {
      title: t('jobRuns.status'),
      dataIndex: 'status',
      key: 'status',
      width: 100,
      render: (value: string) => <Tag color={statusColor(value)}>{value}</Tag>,
    },
    { title: t('jobRuns.input'), dataIndex: 'input_count', key: 'input_count', width: 100 },
    { title: t('jobRuns.output'), dataIndex: 'output_count', key: 'output_count', width: 100 },
    {
      title: t('jobRuns.progress'),
      key: 'progress',
      width: 260,
      render: (_, row) => {
        const before = formatCount(row.pending_before_count);
        const selected = formatCount(row.selected_count);
        const marked = formatCount(row.marked_count);
        const after = formatCount(row.pending_after_count);
        const hasMarked =
          row.marked_count !== undefined && row.marked_count !== null && row.marked_count >= 0;
        const suspect =
          hasMarked &&
          row.marked_count === 0 &&
          row.status === 'done' &&
          row.input_count > 0 &&
          row.job_type === 'summary_l0';
        return (
          <Space direction="vertical" size={0}>
            <span>{`pending ${before} -> ${after}`}</span>
            <span>{`selected ${selected}, marked ${marked}`}</span>
            {suspect && <Tag color="warning">{t('jobRuns.progressSuspect')}</Tag>}
          </Space>
        );
      },
    },
    {
      title: t('jobRuns.startedAt'),
      dataIndex: 'started_at',
      key: 'started_at',
      width: 170,
      render: (value: string) => toLocal(value),
    },
    {
      title: t('jobRuns.finishedAt'),
      dataIndex: 'finished_at',
      key: 'finished_at',
      width: 170,
      render: (value?: string | null) => toLocal(value),
    },
    {
      title: t('jobRuns.duration'),
      key: 'duration',
      width: 110,
      render: (_, row) => formatDuration(row.started_at, row.finished_at),
    },
    {
      title: t('jobRuns.error'),
      dataIndex: 'error_message',
      key: 'error_message',
      ellipsis: true,
      render: (value?: string | null) => {
        if (!value) {
          return '-';
        }
        return (
          <Tooltip title={value}>
            <Text type="danger" ellipsis>
              {value}
            </Text>
          </Tooltip>
        );
      },
    },
  ];

  return (
    <Card
      title={t('jobRuns.title')}
      extra={
        <Space size={12}>
          <span>
            {t('jobRuns.autoRefresh')}{' '}
            <Switch checked={autoRefresh} size="small" onChange={setAutoRefresh} />
          </span>
          <Button onClick={load} loading={loading}>
            {t('common.refresh')}
          </Button>
        </Space>
      }
    >
      {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
      <Row gutter={[12, 12]} style={{ marginBottom: 12 }}>
        <Col xs={12} md={6}>
          <Statistic title={t('jobRuns.total')} value={stats.total} />
        </Col>
        <Col xs={12} md={6}>
          <Statistic title={t('jobRuns.running')} value={stats.running} />
        </Col>
        <Col xs={12} md={6}>
          <Statistic title={t('jobRuns.done')} value={stats.done} />
        </Col>
        <Col xs={12} md={6}>
          <Statistic title={t('jobRuns.failed')} value={stats.failed} />
        </Col>
      </Row>
      <Space wrap style={{ marginBottom: 12 }}>
        <Select
          style={{ width: 220 }}
          value={jobType}
          onChange={setJobType}
          options={jobTypeOptions}
        />
        <Select
          style={{ width: 180 }}
          value={status}
          onChange={setStatus}
          options={[
            { label: t('jobRuns.statusAll'), value: 'all' },
            { label: 'running', value: 'running' },
            { label: 'done', value: 'done' },
            { label: 'failed', value: 'failed' },
          ]}
        />
        <Input
          allowClear
          style={{ width: 260 }}
          value={sessionKeyword}
          onChange={(event) => setSessionKeyword(event.target.value)}
          placeholder={t('jobRuns.sessionSearchPlaceholder')}
        />
      </Space>
      <Table
        rowKey="id"
        loading={loading}
        columns={columns}
        dataSource={visibleItems}
        pagination={{
          pageSize: 12,
          showSizeChanger: true,
          pageSizeOptions: [12, 20, 50, 100],
          showTotal: (total) => `${t('jobRuns.total')}: ${total}`,
        }}
        scroll={{ x: 1400 }}
        size="small"
      />
    </Card>
  );
}
