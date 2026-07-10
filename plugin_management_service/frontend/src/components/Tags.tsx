// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Tag } from 'antd';

import { useI18n } from '../i18n/I18nProvider';
import { runtimeKindLabel } from '../i18n/labels';
import type { RuntimeKind, Visibility } from '../types';

export function VisibilityTag({ value }: { value: Visibility | string }) {
  const { t } = useI18n();
  const color =
    value === 'public' ? 'green' : value === 'system_private' ? 'purple' : 'default';
  return <Tag color={color}>{t(`visibility.${value}`)}</Tag>;
}

export function RuntimeKindTag({ value }: { value: RuntimeKind | string }) {
  const { t } = useI18n();
  const local = value.startsWith('local_connector');
  return (
    <Tag color={local ? 'orange' : value === 'builtin' ? 'blue' : 'cyan'}>
      {runtimeKindLabel(value, t)}
    </Tag>
  );
}

export function EnabledTag({ enabled }: { enabled: boolean }) {
  const { t } = useI18n();
  return <Tag color={enabled ? 'green' : 'red'}>{t(enabled ? 'common.enabled' : 'common.disabled')}</Tag>;
}

export function StatusTag({ status }: { status?: string | null }) {
  const { t } = useI18n();
  const value = status || 'unknown';
  const color = value === 'available' ? 'green' : value === 'unavailable' ? 'red' : 'gold';
  return <Tag color={color}>{t(`status.${value}`)}</Tag>;
}
