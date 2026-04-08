import { Alert, Button, Card, Form, Input, InputNumber, Select, Space, Switch } from 'antd';

import type { RollupJobConfig, TaskExecutionRollupJobConfig } from '../../types';

type RollupConfigLike = RollupJobConfig | TaskExecutionRollupJobConfig;

interface RollupConfigCardProps {
  config: RollupConfigLike | null;
  modelOptions: Array<{ label: string; value: string }>;
  title: string;
  enabledLabel: string;
  modelConfigIdLabel: string;
  summaryPromptLabel: string;
  summaryPromptHint: string;
  resetSummaryPromptLabel: string;
  defaultSummaryPrompt: string;
  roundLimitLabel: string;
  tokenLimitLabel: string;
  targetTokensLabel: string;
  intervalLabel: string;
  keepRawLabel: string;
  maxLevelLabel: string;
  maxCountLabel: string;
  maxCountKey: 'max_sessions_per_tick' | 'max_scopes_per_tick';
  saveLabel: string;
  notConfiguredMessage: string;
  createConfigLabel: string;
  keepRawWarning: string | null;
  triggerHint: string | null;
  onChange: (config: RollupConfigLike) => void;
  onSetNumber: (key: keyof RollupConfigLike, value: number | null, min: number) => void;
  onSave: () => void;
  onCreate: () => void;
}

export function RollupConfigCard({
  config,
  modelOptions,
  title,
  enabledLabel,
  modelConfigIdLabel,
  summaryPromptLabel,
  summaryPromptHint,
  resetSummaryPromptLabel,
  defaultSummaryPrompt,
  roundLimitLabel,
  tokenLimitLabel,
  targetTokensLabel,
  intervalLabel,
  keepRawLabel,
  maxLevelLabel,
  maxCountLabel,
  maxCountKey,
  saveLabel,
  notConfiguredMessage,
  createConfigLabel,
  keepRawWarning,
  triggerHint,
  onChange,
  onSetNumber,
  onSave,
  onCreate,
}: RollupConfigCardProps) {
  return (
    <Card size="small" title={title}>
      {config ? (
        <Form layout="vertical">
          {keepRawWarning ? (
            <Alert type="warning" showIcon message={keepRawWarning} style={{ marginBottom: 12 }} />
          ) : null}
          {triggerHint ? (
            <Alert type="info" showIcon message={triggerHint} style={{ marginBottom: 12 }} />
          ) : null}
          <Form.Item label={enabledLabel}>
            <Switch
              checked={config.enabled === 1}
              onChange={(checked) => onChange({ ...config, enabled: checked ? 1 : 0 })}
            />
          </Form.Item>
          <Form.Item label={modelConfigIdLabel}>
            <Select
              allowClear
              value={config.summary_model_config_id || undefined}
              options={modelOptions}
              onChange={(value) =>
                onChange({
                  ...config,
                  summary_model_config_id: value || null,
                })
              }
            />
          </Form.Item>
          <Form.Item label={summaryPromptLabel} extra={summaryPromptHint}>
            <Space direction="vertical" style={{ width: '100%' }}>
              <Input.TextArea
                value={config.summary_prompt ?? ''}
                autoSize={{ minRows: 3, maxRows: 10 }}
                onChange={(event) =>
                  onChange({
                    ...config,
                    summary_prompt: event.target.value,
                  })
                }
              />
              <Button
                size="small"
                onClick={() =>
                  onChange({
                    ...config,
                    summary_prompt: defaultSummaryPrompt,
                  })
                }
              >
                {resetSummaryPromptLabel}
              </Button>
            </Space>
          </Form.Item>
          <Form.Item label={roundLimitLabel}>
            <InputNumber
              min={3}
              value={config.round_limit}
              onChange={(value) => onSetNumber('round_limit', value, 3)}
              style={{ width: '100%' }}
            />
          </Form.Item>
          <Form.Item label={tokenLimitLabel}>
            <InputNumber
              min={500}
              value={config.token_limit}
              onChange={(value) => onSetNumber('token_limit', value, 500)}
              style={{ width: '100%' }}
            />
          </Form.Item>
          <Form.Item label={targetTokensLabel}>
            <InputNumber
              min={200}
              value={config.target_summary_tokens}
              onChange={(value) => onSetNumber('target_summary_tokens', value, 200)}
              style={{ width: '100%' }}
            />
          </Form.Item>
          <Form.Item label={intervalLabel}>
            <InputNumber
              min={10}
              value={config.job_interval_seconds}
              onChange={(value) => onSetNumber('job_interval_seconds', value, 10)}
              style={{ width: '100%' }}
            />
          </Form.Item>
          <Form.Item label={keepRawLabel}>
            <InputNumber
              min={0}
              value={config.keep_raw_level0_count}
              onChange={(value) => onSetNumber('keep_raw_level0_count', value, 0)}
              style={{ width: '100%' }}
            />
          </Form.Item>
          <Form.Item label={maxLevelLabel}>
            <InputNumber
              min={1}
              value={config.max_level}
              onChange={(value) => onSetNumber('max_level', value, 1)}
              style={{ width: '100%' }}
            />
          </Form.Item>
          <Form.Item label={maxCountLabel}>
            <InputNumber
              min={1}
              value={config[maxCountKey]}
              onChange={(value) => onSetNumber(maxCountKey, value, 1)}
              style={{ width: '100%' }}
            />
          </Form.Item>
          <Button type="primary" onClick={onSave}>
            {saveLabel}
          </Button>
        </Form>
      ) : (
        <Space direction="vertical" style={{ width: '100%' }}>
          <Alert type="info" showIcon message={notConfiguredMessage} />
          <Button type="dashed" onClick={onCreate}>
            {createConfigLabel}
          </Button>
        </Space>
      )}
    </Card>
  );
}
