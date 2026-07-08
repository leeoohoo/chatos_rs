// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';
import { useI18n } from '../../i18n/I18nProvider';
import type { Message } from '../../types';

interface SessionSummaryCardProps {
  message: Message;
  keepLastN: number | null;
}

export const SessionSummaryCard: FC<SessionSummaryCardProps> = ({
  message,
  keepLastN,
}) => {
  const { t } = useI18n();
  const compactedLabel = typeof keepLastN === 'number'
    ? t('sessionSummary.compactedWithCount', { count: keepLastN })
    : t('sessionSummary.compacted');

  return (
    <div className="mb-3 border border-amber-300 dark:border-amber-600/50 bg-amber-50 dark:bg-amber-950/20 rounded-md p-3">
      <div className="text-xs text-amber-900 dark:text-amber-200 font-medium mb-1">
        {compactedLabel}
      </div>
      <details className="group">
        <summary className="cursor-pointer text-xs text-muted-foreground select-none">
          {t('sessionSummary.viewContent')}
        </summary>
        <div className="mt-2 prose prose-sm max-w-none">
          <LazyMarkdownRenderer
            content={(message.rawContent || message.metadata?.summary || '').toString()}
            isStreaming={false}
            onApplyCode={() => {}}
          />
        </div>
      </details>
    </div>
  );
};
