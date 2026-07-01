// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Descriptions,
  Modal,
  Tag,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { RemoteServerTestResponse } from '../../types';

type ServerTestResultModalProps = {
  t: TranslateFn;
  result: RemoteServerTestResponse | null;
  authTypeLabel: (value: string) => string;
  onClose: () => void;
};

export function ServerTestResultModal({
  t,
  result,
  authTypeLabel,
  onClose,
}: ServerTestResultModalProps) {
  return (
    <Modal
      title={t('servers.testResult.title')}
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
        <Descriptions bordered column={1} size="small">
          <Descriptions.Item label={t('servers.testResult.result')}>
            <Tag color={result.ok ? 'success' : 'error'}>
              {result.ok ? t('common.success') : t('common.failed')}
            </Tag>
          </Descriptions.Item>
          <Descriptions.Item label={t('servers.column.server')}>
            {result.name} ({result.username}@{result.host}:{result.port})
          </Descriptions.Item>
          <Descriptions.Item label={t('servers.column.authType')}>
            {authTypeLabel(result.auth_type)}
          </Descriptions.Item>
          <Descriptions.Item label={t('servers.testResult.remoteHost')}>
            {result.remote_host || '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('servers.testResult.error')}>
            {result.error || '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('servers.testResult.testedAt')}>
            {dayjs(result.tested_at).format('YYYY-MM-DD HH:mm:ss')}
          </Descriptions.Item>
        </Descriptions>
      ) : null}
    </Modal>
  );
}
