// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import { isLocalRuntimeSessionId } from '../../lib/api/localRuntime';
import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';
import { useDialogService } from '../ui/DialogProvider';

export interface MemoryTimelineItem {
  id: string;
  sourceId: string;
  kind: 'session_summary' | 'agent_recall';
  text: string;
  time: string;
  sourceLabel: string;
}

export const MemoryTimelineList: React.FC<{
  sessionId: string;
  items: MemoryTimelineItem[];
  onRefresh: () => void;
}> = ({ sessionId, items, onRefresh }) => {
  const client = useApiClient();
  const { t } = useI18n();
  const { confirm } = useDialogService();
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const canForget = isLocalRuntimeSessionId(sessionId);

  const forgetRecall = async (item: MemoryTimelineItem) => {
    const confirmed = await confirm({
      title: t('memory.recallForgetTitle'),
      message: t('memory.recallForgetMessage'),
      confirmText: t('memory.recallForget'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed) return;
    setDeletingId(item.sourceId);
    setError(null);
    try {
      await client.deleteConversationMemoryRecall(sessionId, item.sourceId);
      onRefresh();
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : t('memory.recallForgetFailed'));
    } finally {
      setDeletingId(null);
    }
  };

  if (items.length === 0) {
    return <div className="mt-2 text-xs text-muted-foreground">{t('memory.empty')}</div>;
  }

  return (
    <div className="mt-2 space-y-2">
      {error ? <div className="text-xs text-destructive">{error}</div> : null}
      {items.map((item) => (
        <div key={item.id} className="rounded border border-border p-2">
          <div className="flex items-center justify-between gap-2 text-[11px] text-muted-foreground">
            <span>{item.sourceLabel}</span>
            <div className="flex items-center gap-2">
              <span>{formatTextDate(item.time)}</span>
              {canForget && item.kind === 'agent_recall' ? (
                <button
                  type="button"
                  className="text-destructive hover:underline disabled:opacity-60"
                  disabled={deletingId === item.sourceId}
                  onClick={() => void forgetRecall(item)}
                >
                  {deletingId === item.sourceId
                    ? t('memory.recallForgetting')
                    : t('memory.recallForget')}
                </button>
              ) : null}
            </div>
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
