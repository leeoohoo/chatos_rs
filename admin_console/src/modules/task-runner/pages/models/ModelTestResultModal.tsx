// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Descriptions,
  Modal,
  Space,
  Tag,
  Typography,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { ModelConfigTestResponse } from '../../types';

type ModelTestResultModalProps = {
  t: TranslateFn;
  result: ModelConfigTestResponse | null;
  onClose: () => void;
};

export function ModelTestResultModal({
  t,
  result,
  onClose,
}: ModelTestResultModalProps) {
  return (
    <Modal
      title={t('models.testResult.title')}
      open={Boolean(result)}
      width={680}
      footer={[
        <Button key="close" onClick={onClose}>
          {t('common.close')}
        </Button>,
      ]}
      onCancel={onClose}
    >
      {result ? (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label={t('models.testResult.result')}>
              <Tag color={result.ok ? 'success' : 'error'}>
                {result.ok ? t('common.success') : t('common.failed')}
              </Tag>
            </Descriptions.Item>
            <Descriptions.Item label="Provider">{result.provider}</Descriptions.Item>
            <Descriptions.Item label="Model">{result.model}</Descriptions.Item>
            <Descriptions.Item label={t('models.testResult.testedAt')}>
              {dayjs(result.tested_at).format('YYYY-MM-DD HH:mm:ss')}
            </Descriptions.Item>
            <Descriptions.Item label="Response ID">
              {result.response_id || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('models.testResult.output')}>
              {result.content || '-'}
            </Descriptions.Item>
            <Descriptions.Item label="Reasoning">
              {result.reasoning || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('models.testResult.error')}>
              {result.error || '-'}
            </Descriptions.Item>
          </Descriptions>
          {result.usage ? (
            <Typography.Paragraph
              style={{
                background: '#fafafa',
                padding: 12,
                borderRadius: 6,
                marginBottom: 0,
                whiteSpace: 'pre-wrap',
                fontFamily: 'monospace',
              }}
            >
              {JSON.stringify(result.usage, null, 2)}
            </Typography.Paragraph>
          ) : null}
        </Space>
      ) : null}
    </Modal>
  );
}
