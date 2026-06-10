import React from 'react';
import type { UiPromptHistoryItem } from './types';
import { useI18n, type TranslateFn } from '../../i18n/I18nProvider';

interface UiPromptHistoryDrawerProps {
  open: boolean;
  items: UiPromptHistoryItem[];
  loading: boolean;
  error: string | null;
  refreshDisabled: boolean;
  onRefresh: () => void;
  onClose: () => void;
  formatCreatedAt: (value: string) => string;
}

const formatUiPromptStatus = (status: string, t: TranslateFn): string => {
  const normalized = String(status || '').trim().toLowerCase();
  if (normalized === 'ok') return t('uiPromptHistory.status.ok');
  if (normalized === 'canceled' || normalized === 'cancelled') return t('uiPromptHistory.status.canceled');
  if (normalized === 'timeout') return t('uiPromptHistory.status.timeout');
  if (normalized === 'pending') return t('uiPromptHistory.status.pending');
  return normalized || '-';
};

const uiPromptStatusClass = (status: string): string => {
  const normalized = String(status || '').trim().toLowerCase();
  if (normalized === 'ok') {
    return 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-200';
  }
  if (normalized === 'canceled' || normalized === 'cancelled') {
    return 'bg-slate-200 text-slate-700 dark:bg-slate-700 dark:text-slate-200';
  }
  if (normalized === 'timeout') {
    return 'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-200';
  }
  if (normalized === 'pending') {
    return 'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-200';
  }
  return 'bg-muted text-muted-foreground';
};

const UiPromptHistoryDrawer: React.FC<UiPromptHistoryDrawerProps> = ({
  open,
  items,
  loading,
  error,
  refreshDisabled,
  onRefresh,
  onClose,
  formatCreatedAt,
}) => {
  const { t } = useI18n();

  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50">
      <button
        type="button"
        aria-label={t('uiPromptHistory.closeAria')}
        className="absolute inset-0 bg-black/35"
        onClick={onClose}
      />
      <div className="absolute right-0 top-0 h-full w-full max-w-xl border-l border-border bg-card shadow-xl">
        <div className="flex h-full flex-col">
          <div className="flex items-center justify-between border-b border-border px-4 py-3">
            <div>
              <div className="text-sm font-semibold text-foreground">{t('uiPromptHistory.title')}</div>
              <div className="text-xs text-muted-foreground">
                {t('uiPromptHistory.currentSessionCount', { count: items.length })}
              </div>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
                disabled={refreshDisabled}
                onClick={onRefresh}
              >
                {loading ? t('common.refreshing') : t('common.refresh')}
              </button>
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent"
                onClick={onClose}
              >
                {t('common.close')}
              </button>
            </div>
          </div>

          <div className="custom-scrollbar flex-1 overflow-y-scroll px-3 py-3 [scrollbar-gutter:stable]">
            {error ? (
              <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs text-destructive">
                {error}
              </div>
            ) : null}

            {loading && items.length === 0 ? (
              <div className="text-xs text-muted-foreground">{t('uiPromptHistory.loading')}</div>
            ) : null}

            {!loading && items.length === 0 ? (
              <div className="text-xs text-muted-foreground">{t('uiPromptHistory.empty')}</div>
            ) : null}

            {items.length > 0 ? (
              <div className="space-y-2">
                {items.map((item) => (
                  <div key={item.id} className="rounded-lg border border-border bg-background/80 p-3">
                    <div className="flex items-center justify-between gap-2">
                      <div className="truncate text-sm font-medium text-foreground">
                        {item.title || t('uiPromptHistory.unnamedPrompt')}
                      </div>
                      <span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${uiPromptStatusClass(item.status)}`}>
                        {formatUiPromptStatus(item.status, t)}
                      </span>
                    </div>
                    {item.message ? (
                      <div className="mt-1 line-clamp-2 text-xs text-muted-foreground">{item.message}</div>
                    ) : null}
                    <div className="mt-1 text-[11px] text-muted-foreground">
                      {t('uiPromptHistory.meta', {
                        kind: item.kind || '-',
                        time: formatCreatedAt(item.updatedAt || item.createdAt),
                      })}
                    </div>
                    {item.response ? (
                      <pre className="custom-scrollbar mt-2 max-h-40 overflow-y-scroll rounded border border-border bg-background p-2 text-[11px] text-foreground [scrollbar-gutter:stable]">
{JSON.stringify(item.response, null, 2)}
                      </pre>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
};

export default UiPromptHistoryDrawer;
