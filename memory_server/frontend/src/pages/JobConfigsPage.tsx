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
  Table,
} from 'antd';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { RollupJobConfig, SummaryJobConfig, UserItem } from '../types';

interface JobConfigsPageProps {
  userId: string;
  isAdmin: boolean;
  selectedSessionId?: string;
  onSelectUser?: (userId: string) => void;
  showUserSelector?: boolean;
}

export function JobConfigsPage({
  userId,
  isAdmin,
  selectedSessionId,
  onSelectUser,
  showUserSelector = true,
}: JobConfigsPageProps) {
  const { t } = useI18n();
  const [targetUserId, setTargetUserId] = useState(userId);
  const [users, setUsers] = useState<UserItem[]>([]);
  const [usersLoading, setUsersLoading] = useState(false);
  const [summaryCfg, setSummaryCfg] = useState<SummaryJobConfig | null>(null);
  const [rollupCfg, setRollupCfg] = useState<RollupJobConfig | null>(null);
  const [modelOptions, setModelOptions] = useState<Array<{ label: string; value: string }>>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    setTargetUserId(userId);
  }, [userId]);

  useEffect(() => {
    const uid = targetUserId.trim();
    if (uid) {
      onSelectUser?.(uid);
    }
  }, [targetUserId, onSelectUser]);

  const createSummaryConfig = (uid: string): SummaryJobConfig => ({
    user_id: uid,
    enabled: 1,
    summary_model_config_id: null,
    token_limit: 6000,
    round_limit: 8,
    target_summary_tokens: 700,
    job_interval_seconds: 30,
    max_sessions_per_tick: 50,
  });

  const createRollupConfig = (uid: string): RollupJobConfig => ({
    user_id: uid,
    enabled: 1,
    summary_model_config_id: null,
    token_limit: 6000,
    round_limit: 50,
    target_summary_tokens: 700,
    job_interval_seconds: 60,
    keep_raw_level0_count: 5,
    max_level: 4,
    max_sessions_per_tick: 50,
  });

  const loadUsers = async () => {
    setUsersLoading(true);
    try {
      const items = await api.listUsers(500);
      setUsers(items);
      if (items.length === 0) {
        return;
      }
      const currentTarget = targetUserId.trim();
      if (currentTarget && items.some((item) => item.username === currentTarget)) {
        return;
      }
      const preferred = userId.trim();
      if (preferred && items.some((item) => item.username === preferred)) {
        setTargetUserId(preferred);
        return;
      }
      setTargetUserId(items[0].username);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setUsersLoading(false);
    }
  };

  const disabled = useMemo(() => !targetUserId.trim(), [targetUserId]);

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
      const uid = targetUserId.trim();
      const [s, r, models] = await Promise.all([
        api.getSummaryJobConfig(uid),
        api.getRollupJobConfig(uid),
        api.listModelConfigs(uid),
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
    if (!showUserSelector) {
      return;
    }
    loadUsers();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isAdmin, showUserSelector]);

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [targetUserId]);

  const saveSummary = async () => {
    if (!summaryCfg) {
      return;
    }
    setError(null);
    setMessage(null);
    try {
      const saved = await api.saveSummaryJobConfig({
        ...summaryCfg,
        user_id: targetUserId.trim(),
      });
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
      const saved = await api.saveRollupJobConfig({
        ...rollupCfg,
        user_id: targetUserId.trim(),
      });
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
      const data = await api.runSummaryOnce(targetUserId.trim(), selectedSessionId);
      setMessage(JSON.stringify(data));
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const runRollupNow = async () => {
    setError(null);
    setMessage(null);
    try {
      const data = await api.runRollupOnce(targetUserId.trim());
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
            {showUserSelector && (
              <Button onClick={loadUsers} loading={usersLoading}>
                {t('common.refresh')}
              </Button>
            )}
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
        {showUserSelector && (
          <Card size="small" title={t('jobConfigs.userListTitle')} style={{ marginBottom: 12 }}>
            <Table<UserItem>
              rowKey="username"
              loading={usersLoading}
              dataSource={users}
              pagination={false}
              size="small"
              columns={[
                {
                  title: t('top.userId'),
                  dataIndex: 'username',
                  key: 'username',
                },
                {
                  title: t('top.role'),
                  dataIndex: 'role',
                  key: 'role',
                },
                {
                  title: t('common.action'),
                  key: 'action',
                  width: 160,
                  render: (_, record) => (
                    <Button
                      type={targetUserId === record.username ? 'primary' : 'default'}
                      onClick={() => {
                        setTargetUserId(record.username);
                        onSelectUser?.(record.username);
                      }}
                    >
                      {t('jobConfigs.viewConfig')}
                    </Button>
                  ),
                },
              ]}
            />
          </Card>
        )}

        {showUserSelector && (
          <Alert
            type="info"
            showIcon
            message={`${t('jobConfigs.currentTarget')}: ${targetUserId || '-'}`}
            style={{ marginBottom: 12 }}
          />
        )}
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
                {!summaryCfg && (
                  <Space direction="vertical" style={{ width: '100%' }}>
                    <Alert type="info" showIcon message={t('jobConfigs.notConfiguredSummary')} />
                    <Button
                      type="dashed"
                      onClick={() => {
                        const uid = targetUserId.trim();
                        if (!uid) {
                          return;
                        }
                        setSummaryCfg(createSummaryConfig(uid));
                      }}
                    >
                      {t('jobConfigs.createSummaryConfig')}
                    </Button>
                  </Space>
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
                        min={3}
                        value={rollupCfg.round_limit}
                        onChange={(value) => setRollupNumber('round_limit', value, 3)}
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
                {!rollupCfg && (
                  <Space direction="vertical" style={{ width: '100%' }}>
                    <Alert type="info" showIcon message={t('jobConfigs.notConfiguredRollup')} />
                    <Button
                      type="dashed"
                      onClick={() => {
                        const uid = targetUserId.trim();
                        if (!uid) {
                          return;
                        }
                        setRollupCfg(createRollupConfig(uid));
                      }}
                    >
                      {t('jobConfigs.createRollupConfig')}
                    </Button>
                  </Space>
                )}
              </Card>
            </Col>
          </Row>
        )}
      </Card>
    </Space>
  );
}
