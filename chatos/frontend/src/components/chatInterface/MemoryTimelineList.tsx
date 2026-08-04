// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';

export interface MemoryTimelineItem {
  id: string;
  sourceId: string;
  kind: 'session_summary' | 'agent_recall';
  text: string;
  time: string;
  sourceLabel: string;
}

export const MemoryTimelineList: React.FC<{
  items: MemoryTimelineItem[];
}> = ({ items }) => {
  const { t } = useI18n();

  if (items.length === 0) {
    return <div className="mt-2 text-xs text-muted-foreground">{t('memory.empty')}</div>;
  }

  return (
    <div className="mt-2 space-y-2">
      {items.map((item) => (
        <div key={item.id} className="rounded border border-border p-2">
          <div className="flex items-center justify-between gap-2 text-[11px] text-muted-foreground">
            <span>{item.sourceLabel}</span>
            <span>{formatTextDate(item.time)}</span>
          </div>
          <div className="mt-1 text-sm leading-6">
            <LazyMarkdownRenderer content={item.text} />
          </div>
        </div>
      ))}
    </div>
  );
};

const formatTextDate = (value?: string | null): string => {
  if (!value) return '-';
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
};
