import { useEffect, useState } from 'react';
import { Alert, Button, Card, Col, Empty, Row, Spin, Statistic, Tag } from 'antd';

import { api } from '../api/client';
import { useI18n } from '../i18n';

type JobStats = Record<string, Record<string, number>>;

export function DashboardPage() {
  const { t } = useI18n();
  const [stats, setStats] = useState<JobStats>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.getJobStats();
      setStats(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
  }, []);

  return (
    <Card
      title={t('dashboard.title')}
      extra={
        <Button onClick={load} loading={loading}>
          {t('common.refresh')}
        </Button>
      }
    >
      {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
      {loading ? (
        <Spin />
      ) : Object.keys(stats).length === 0 ? (
        <Empty description={t('dashboard.empty')} />
      ) : (
        <Row gutter={[12, 12]}>
          {Object.entries(stats).map(([jobType, statuses]) => (
            <Col key={jobType} xs={24} lg={12}>
              <Card size="small" title={<Tag color="blue">{jobType}</Tag>}>
                <Row gutter={[12, 12]}>
                  {Object.entries(statuses).map(([status, count]) => (
                    <Col key={status} xs={12}>
                      <Statistic title={status} value={count} />
                    </Col>
                  ))}
                </Row>
              </Card>
            </Col>
          ))}
        </Row>
      )}
    </Card>
  );
}
