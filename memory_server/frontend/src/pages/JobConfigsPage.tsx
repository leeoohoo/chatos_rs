import { useEffect, useMemo, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Col,
  Form,
  InputNumber,
  Row,
  Select,
  Space,
  Spin,
  Switch,
} from 'antd';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { RollupJobConfig, SummaryJobConfig } from '../types';

interface JobConfigsPageProps {
  userId: string;
  selectedSessionId?: string;
}

export function JobConfigsPage({ userId, selectedSessionId }: JobConfigsPageProps) {
  const { t } = useI18n();
  const [summaryCfg, setSummaryCfg] = useState<SummaryJobConfig | null>(null);
  const [rollupCfg, setRollupCfg] = useState<RollupJobConfig | null>(null);
  const [modelOptions, setModelOptions] = useState<Array<{ label: string; value: string }>>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const disabled = useMemo(() => !userId.trim(), [userId]);

  const load = async () => {
    if (disabled) {
      setSummaryCfg(null);
      setRollupCfg(null);
      setModelOptions([]);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const [s, r, models] = await Promise.all([
        api.getSummaryJobConfig(userId),
        api.getRollupJobConfig(userId),
        api.listModelConfigs(userId),
      ]);
      setSummaryCfg(s);
      setRollupCfg(r);
      setModelOptions(
        models.map((item) => ({
          label: `${item.name} (${item.provider}/${item.model})`,
          value: item.id,
        })),
      );
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [userId]);

  const saveSummary = async () => {
    if (!summaryCfg) {
      return;
    }
    setError(null);
    setMessage(null);
    try {
      const saved = await api.saveSummaryJobConfig(summaryCfg);
      setSummaryCfg(saved);
      setMessage(t('jobConfigs.saved'));
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const saveRollup = async () => {
    if (!rollupCfg) {
      return;
    }
    setError(null);
    setMessage(null);
    try {
      const saved = await api.saveRollupJobConfig(rollupCfg);
      setRollupCfg(saved);
      setMessage(t('jobConfigs.saved'));
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const runSummaryNow = async () => {
    setError(null);
    setMessage(null);
    try {
      const data = await api.runSummaryOnce(userId, selectedSessionId);
      setMessage(JSON.stringify(data));
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const runRollupNow = async () => {
    setError(null);
    setMessage(null);
    try {
      const data = await api.runRollupOnce(userId);
      setMessage(JSON.stringify(data));
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const setSummaryNumber = (key: keyof SummaryJobConfig, value: number | null, min: number) => {
    if (!summaryCfg) {
      return;
    }
    const next = value === null ? min : Math.max(min, Math.floor(value));
    setSummaryCfg({ ...summaryCfg, [key]: next });
  };

  const setRollupNumber = (key: keyof RollupJobConfig, value: number | null, min: number) => {
    if (!rollupCfg) {
      return;
    }
    const next = value === null ? min : Math.max(min, Math.floor(value));
    setRollupCfg({ ...rollupCfg, [key]: next });
  };

  return (
    <Space direction="vertical" size={12} style={{ width: '100%' }}>
      <Card
        title={t('jobConfigs.title')}
        extra={
          <Space>
            <Button onClick={load} loading={loading}>
              {t('common.refresh')}
            </Button>
            <Button type="primary" onClick={runSummaryNow} disabled={disabled}>
              {t('jobConfigs.runSummaryNow')}
            </Button>
            <Button onClick={runRollupNow} disabled={disabled}>
              {t('jobConfigs.runRollupNow')}
            </Button>
          </Space>
        }
      >
        {disabled && (
          <Alert type="warning" showIcon message={t('sessions.needUserId')} style={{ marginBottom: 12 }} />
        )}
        {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
        {message && <Alert type="success" showIcon message={message} style={{ marginBottom: 12 }} />}

        {loading && !summaryCfg && !rollupCfg ? (
          <Spin />
        ) : (
          <Row gutter={[12, 12]}>
            <Col xs={24} xl={12}>
              <Card size="small" title={t('jobConfigs.summaryConfig')}>
                {summaryCfg && (
                  <Form layout="vertical">
                    <Form.Item label={t('common.enabled')}>
                      <Switch
                        checked={summaryCfg.enabled === 1}
                        onChange={(checked) =>
                          setSummaryCfg({ ...summaryCfg, enabled: checked ? 1 : 0 })
                        }
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.modelConfigId')}>
                      <Select
                        allowClear
                        value={summaryCfg.summary_model_config_id || undefined}
                        options={modelOptions}
                        onChange={(value) =>
                          setSummaryCfg({
                            ...summaryCfg,
                            summary_model_config_id: value || null,
                          })
                        }
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.roundLimit')}>
                      <InputNumber
                        min={1}
                        value={summaryCfg.round_limit}
                        onChange={(value) => setSummaryNumber('round_limit', value, 1)}
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.tokenLimit')}>
                      <InputNumber
                        min={500}
                        value={summaryCfg.token_limit}
                        onChange={(value) => setSummaryNumber('token_limit', value, 500)}
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.targetTokens')}>
                      <InputNumber
                        min={200}
                        value={summaryCfg.target_summary_tokens}
                        onChange={(value) =>
                          setSummaryNumber('target_summary_tokens', value, 200)
                        }
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.interval')}>
                      <InputNumber
                        min={10}
                        value={summaryCfg.job_interval_seconds}
                        onChange={(value) =>
                          setSummaryNumber('job_interval_seconds', value, 10)
                        }
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.maxSessions')}>
                      <InputNumber
                        min={1}
                        value={summaryCfg.max_sessions_per_tick}
                        onChange={(value) =>
                          setSummaryNumber('max_sessions_per_tick', value, 1)
                        }
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Button type="primary" onClick={saveSummary}>
                      {t('common.save')}
                    </Button>
                  </Form>
                )}
              </Card>
            </Col>

            <Col xs={24} xl={12}>
              <Card size="small" title={t('jobConfigs.rollupConfig')}>
                {rollupCfg && (
                  <Form layout="vertical">
                    <Form.Item label={t('common.enabled')}>
                      <Switch
                        checked={rollupCfg.enabled === 1}
                        onChange={(checked) =>
                          setRollupCfg({ ...rollupCfg, enabled: checked ? 1 : 0 })
                        }
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.modelConfigId')}>
                      <Select
                        allowClear
                        value={rollupCfg.summary_model_config_id || undefined}
                        options={modelOptions}
                        onChange={(value) =>
                          setRollupCfg({
                            ...rollupCfg,
                            summary_model_config_id: value || null,
                          })
                        }
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.roundLimit')}>
                      <InputNumber
                        min={10}
                        value={rollupCfg.round_limit}
                        onChange={(value) => setRollupNumber('round_limit', value, 10)}
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.tokenLimit')}>
                      <InputNumber
                        min={500}
                        value={rollupCfg.token_limit}
                        onChange={(value) => setRollupNumber('token_limit', value, 500)}
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.targetTokens')}>
                      <InputNumber
                        min={200}
                        value={rollupCfg.target_summary_tokens}
                        onChange={(value) =>
                          setRollupNumber('target_summary_tokens', value, 200)
                        }
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.interval')}>
                      <InputNumber
                        min={10}
                        value={rollupCfg.job_interval_seconds}
                        onChange={(value) =>
                          setRollupNumber('job_interval_seconds', value, 10)
                        }
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.keepRaw')}>
                      <InputNumber
                        min={0}
                        value={rollupCfg.keep_raw_level0_count}
                        onChange={(value) =>
                          setRollupNumber('keep_raw_level0_count', value, 0)
                        }
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.maxLevel')}>
                      <InputNumber
                        min={1}
                        value={rollupCfg.max_level}
                        onChange={(value) => setRollupNumber('max_level', value, 1)}
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Form.Item label={t('jobConfigs.maxSessions')}>
                      <InputNumber
                        min={1}
                        value={rollupCfg.max_sessions_per_tick}
                        onChange={(value) =>
                          setRollupNumber('max_sessions_per_tick', value, 1)
                        }
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                    <Button type="primary" onClick={saveRollup}>
                      {t('common.save')}
                    </Button>
                  </Form>
                )}
              </Card>
            </Col>
          </Row>
        )}
      </Card>
    </Space>
  );
}
