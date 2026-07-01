// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Tag } from 'antd';

import { useI18n } from '../i18n';
import type { SandboxStatus } from '../types';

const colorByStatus: Record<SandboxStatus, string> = {
  pending: 'default',
  leasing: 'processing',
  starting: 'processing',
  ready: 'success',
  running: 'blue',
  releasing: 'warning',
  destroying: 'warning',
  destroyed: 'default',
  failed: 'error',
  expired: 'error',
};

export function StatusTag({ status }: { status: SandboxStatus }) {
  const { t } = useI18n();
  return <Tag color={colorByStatus[status]}>{t(`status.${status}`)}</Tag>;
}
