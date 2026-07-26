// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type { MessageTaskRunnerRunEvent } from '../../lib/api/client/types';
import { cn } from '../../lib/utils';
import { pluginRuntimeSummariesFromEvents } from './pluginRuntimeEvents';
import { formatDateTime } from './utils';

const statusTone = (status: string): string => {
  const normalized = status.toLowerCase();
  if (normalized.includes('fail') || normalized.includes('error') || normalized === 'blocked') {
    return 'border-red-500/30 bg-red-500/10 text-red-700 dark:text-red-300';
  }
  if (
    normalized.includes('success')
    || normalized.includes('complete')
    || normalized === 'ready'
    || normalized === 'healthy'
  ) {
    return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300';
  }
  if (normalized.includes('cancel') || normalized.includes('expired')) {
    return 'border-border bg-muted text-muted-foreground';
  }
  return 'border-blue-500/30 bg-blue-500/10 text-blue-700 dark:text-blue-300';
};

export const PluginRuntimeEventsCard: FC<{ events: MessageTaskRunnerRunEvent[] }> = ({ events }) => {
  const { t } = useI18n();
  const activities = pluginRuntimeSummariesFromEvents(events).slice(-50).reverse();
  if (!activities.length) {
    return null;
  }
  return (
    <div className="mb-4 rounded-lg border border-violet-500/30 bg-violet-500/5 p-3">
      <div className="mb-1 flex flex-wrap items-center justify-between gap-2">
        <div className="text-sm font-medium">{t('pluginRuntime.runPanelTitle')}</div>
        <span className="text-[11px] text-muted-foreground">
          {t('pluginRuntime.eventCount', { count: activities.length })}
        </span>
      </div>
      <div className="mb-3 text-[11px] text-muted-foreground">
        {t('pluginRuntime.privacyNotice')}
      </div>
      <div className="max-h-96 space-y-2 overflow-y-auto pr-1">
        {activities.map((activity) => {
          const identity = [activity.pluginId, activity.componentKey].filter(Boolean).join(' / ');
          const details = [
            activity.operation,
            activity.toolName ? `tool=${activity.toolName}` : null,
            activity.healthStatus ? `health=${activity.healthStatus}` : null,
            activity.durationMs === null ? null : `${activity.durationMs} ms`,
          ].filter(Boolean);
          return (
            <div key={activity.key} className="rounded-md border bg-background px-3 py-2">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-mono text-xs font-medium">
                  {identity || t('pluginRuntime.unknownPlugin')}
                </span>
                <span className="text-[11px] text-muted-foreground">{activity.phase}</span>
                <span className={cn(
                  'rounded border px-1.5 py-0.5 text-[10px] font-medium',
                  statusTone(activity.status),
                )}
                >
                  {activity.status}
                </span>
                {activity.createdAt ? (
                  <span className="ml-auto text-[10px] text-muted-foreground">
                    {formatDateTime(activity.createdAt)}
                  </span>
                ) : null}
              </div>
              {details.length ? (
                <div className="mt-1 break-all font-mono text-[10px] text-muted-foreground">
                  {details.join(' · ')}
                </div>
              ) : null}
              {activity.releaseId || activity.sessionId ? (
                <div className="mt-1 break-all text-[10px] text-muted-foreground">
                  {[activity.releaseId, activity.sessionId].filter(Boolean).join(' · ')}
                </div>
              ) : null}
              {activity.hook ? (
                <div className="mt-2 flex flex-wrap gap-1.5 text-[10px] text-muted-foreground">
                  <span>{activity.hook.event || t('pluginRuntime.hook')}</span>
                  {activity.hook.executions > 0 ? (
                    <span>{t('pluginRuntime.hookCounts', {
                      matched: activity.hook.matched,
                      failed: activity.hook.failed,
                      timedOut: activity.hook.timedOut,
                    })}</span>
                  ) : null}
                  {activity.hook.workspaceWriteRequested > 0 ? (
                    <span>{t('pluginRuntime.workspaceWriteCounts', {
                      approved: activity.hook.workspaceWriteApproved,
                      denied: activity.hook.workspaceWriteDenied,
                    })}</span>
                  ) : null}
                </div>
              ) : null}
              {activity.error ? (
                <div className="mt-2 break-words rounded bg-red-500/5 px-2 py-1 text-[11px] text-red-700 dark:text-red-300">
                  {activity.error}
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </div>
  );
};
