// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Button, Empty, Modal, Typography } from 'antd';

import { McpPromptPreviewCard } from '../../components/McpPromptPreviewCard';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type { McpPromptPreviewResponse } from '../../types';

type TaskMcpPromptPreviewModalProps = {
  t: TranslateFn;
  title: string;
  open: boolean;
  preview?: McpPromptPreviewResponse;
  loading: boolean;
  onClose: () => void;
};

export function TaskMcpPromptPreviewModal({
  t,
  title,
  open,
  preview,
  loading,
  onClose,
}: TaskMcpPromptPreviewModalProps) {
  return (
    <Modal
      title={title}
      open={open}
      width={860}
      footer={[
        <Button key="close" onClick={onClose}>
          {t('common.close')}
        </Button>,
      ]}
      onCancel={onClose}
    >
      {preview ? (
        <McpPromptPreviewCard preview={preview} />
      ) : loading ? (
        <Typography.Text type="secondary">{t('tasks.preview.loading')}</Typography.Text>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.preview.unavailable')} />
      )}
    </Modal>
  );
}
