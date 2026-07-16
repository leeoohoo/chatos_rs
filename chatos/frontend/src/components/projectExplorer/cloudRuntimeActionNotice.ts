// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { TranslateFn } from '../../i18n/I18nProvider';

export interface RuntimeActionNotice {
  tone: 'info' | 'success' | 'warning';
  message: string;
}

export function actionNoticeForRuntimeStatus(
  status: string,
  t: TranslateFn,
  summary?: string,
): RuntimeActionNotice {
  if (status === 'pending_configuration') {
    return {
      tone: 'warning',
      message: summary
        ? t('cloudRuntime.configurationChecked', { detail: summary })
        : t('cloudRuntime.configurationRequired'),
    };
  }
  if (status === 'ready') {
    return {
      tone: 'success',
      message: t('cloudRuntime.initializationCompleted'),
    };
  }
  return {
    tone: 'info',
    message: t('cloudRuntime.requestAccepted'),
  };
}
