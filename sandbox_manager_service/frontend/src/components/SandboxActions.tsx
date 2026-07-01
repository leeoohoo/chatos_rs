// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { DeleteOutlined, ExportOutlined } from '@ant-design/icons';
import { App, Button, Popconfirm, Space } from 'antd';
import { useMutation, useQueryClient } from '@tanstack/react-query';

import { sandboxesApi } from '../api/sandboxes';
import { useI18n } from '../i18n';
import type { SandboxLeaseRecord } from '../types';

interface SandboxActionsProps {
  sandbox: SandboxLeaseRecord;
  size?: 'small' | 'middle';
}

export function SandboxActions({ sandbox, size = 'small' }: SandboxActionsProps) {
  const queryClient = useQueryClient();
  const { message } = App.useApp();
  const { t } = useI18n();

  const releaseMutation = useMutation({
    mutationFn: () => sandboxesApi.release(sandbox.sandbox_id, sandbox.id),
    onSuccess: async () => {
      message.success(t('actions.releaseSuccess'));
      await queryClient.invalidateQueries({ queryKey: ['sandboxes'] });
      await queryClient.invalidateQueries({ queryKey: ['sandbox', sandbox.sandbox_id] });
      await queryClient.invalidateQueries({ queryKey: ['pool-status'] });
    },
    onError: (error) => message.error(error instanceof Error ? error.message : t('actions.releaseFailure')),
  });

  const destroyMutation = useMutation({
    mutationFn: () => sandboxesApi.destroy(sandbox.sandbox_id),
    onSuccess: async () => {
      message.success(t('actions.destroySuccess'));
      await queryClient.invalidateQueries({ queryKey: ['sandboxes'] });
      await queryClient.invalidateQueries({ queryKey: ['sandbox', sandbox.sandbox_id] });
      await queryClient.invalidateQueries({ queryKey: ['pool-status'] });
    },
    onError: (error) => message.error(error instanceof Error ? error.message : t('actions.destroyFailure')),
  });

  const disabled = sandbox.status === 'destroyed';

  return (
    <Space size={6}>
      <Popconfirm
        title={t('actions.releaseTitle')}
        description={t('actions.releaseDescription')}
        okText={t('common.release')}
        cancelText={t('common.cancel')}
        onConfirm={() => releaseMutation.mutate()}
        disabled={disabled}
      >
        <Button
          size={size}
          icon={<ExportOutlined />}
          loading={releaseMutation.isPending}
          disabled={disabled}
        >
          {t('common.release')}
        </Button>
      </Popconfirm>
      <Popconfirm
        title={t('actions.destroyTitle')}
        description={t('actions.destroyDescription')}
        okText={t('common.destroy')}
        cancelText={t('common.cancel')}
        onConfirm={() => destroyMutation.mutate()}
        disabled={disabled}
      >
        <Button
          danger
          size={size}
          icon={<DeleteOutlined />}
          loading={destroyMutation.isPending}
          disabled={disabled}
        >
          {t('common.destroy')}
        </Button>
      </Popconfirm>
    </Space>
  );
}
