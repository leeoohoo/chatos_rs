import { Alert, Button, Card, Form, Input, InputNumber, Select, Space, Switch } from 'antd';

import type { SummaryJobConfig, TaskExecutionSummaryJobConfig } from '../../types';
import { DEFAULT_SUMMARY_PROMPT_TEMPLATE } from './helpers';

type SummaryConfigLike = SummaryJobConfig | TaskExecutionSummaryJobConfig;

interface SummaryConfigCardProps {
  config: SummaryConfigLike | null;
  modelOptions: Array<{ label: string; value: string }>;
  title: string;
  enabledLabel: string;
  modelConfigIdLabel: string;
  summaryPromptLabel: string;
  summaryPromptHint: string;
  resetSummaryPromptLabel: string;
  roundLimitLabel: string;
  tokenLimitLabel: string;
  targetTokensLabel: string;
  intervalLabel: string;
  maxCountLabel: string;
  maxCountKey: 'max_sessions_per_tick' | 'max_scopes_per_tick';
  saveLabel: string;
  notConfiguredMessage: string;
  createConfigLabel: string;
  onChange: (config: SummaryConfigLike) => void;
  onSetNumber: (key: keyof SummaryConfigLike, value: number | null, min: number) => void;
  onSave: () => void;
  onCreate: () => void;
}

export function SummaryConfigCard({
  config,
  modelOptions,
  title,
  enabledLabel,
  modelConfigIdLabel,
  summaryPromptLabel,
  summaryPromptHint,
  resetSummaryPromptLabel,
  roundLimitLabel,
  tokenLimitLabel,
  targetTokensLabel,
  intervalLabel,
  maxCountLabel,
  maxCountKey,
  saveLabel,
  notConfiguredMessage,
  createConfigLabel,
  onChange,
  onSetNumber,
  onSave,
  onCreate,
}: SummaryConfigCardProps) {
  return (
    <Card size="small" title={title}>
      {config ? (
        <Form layout="vertical">
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
                    summary_prompt: DEFAULT_SUMMARY_PROMPT_TEMPLATE,
                  })
                }
              >
                {resetSummaryPromptLabel}
              </Button>
            </Space>
          </Form.Item>
          <Form.Item label={roundLimitLabel}>
            <InputNumber
              min={1}
              value={config.round_limit}
              onChange={(value) => onSetNumber('round_limit', value, 1)}
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
