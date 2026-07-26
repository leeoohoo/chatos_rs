// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { localRuntimeBridgeAvailable } from '../../lib/api/localRuntime';
import { openBrowserSessionPanel, type BrowserSessionUiTarget } from '../../lib/browserSessionUi';
import type { MessageTaskRunnerRunEvent } from '../../lib/api/client/types';

const record = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

const string = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');

export const browserSessionTargetsFromEvents = (
  events: MessageTaskRunnerRunEvent[],
): BrowserSessionUiTarget[] => {
  const sessions = new Map<string, BrowserSessionUiTarget>();
  for (const event of events) {
    if (string(event.event_type).toLowerCase() !== 'browser_session') {
      continue;
    }
    const payload = record(event.payload);
    const session = record(payload?.browser_session) || payload;
    const id = string(session?.id);
    const mode = string(session?.mode);
    const workspaceId = string(session?.workspace_id ?? session?.workspaceId);
    if (!id || mode !== 'managed' || !workspaceId) {
      continue;
    }
    sessions.set(id, {
      id,
      mode: 'managed',
      workspaceId,
      deviceId: string(session?.device_id ?? session?.deviceId) || null,
      projectId: string(session?.project_id ?? session?.projectId) || null,
      status: string(session?.status) || null,
      url: string(session?.url) || null,
      title: string(session?.title) || null,
    });
  }
  return [...sessions.values()];
};

export const BrowserSessionEventsCard: FC<{ events: MessageTaskRunnerRunEvent[] }> = ({ events }) => {
  const { t } = useI18n();
  const sessions = browserSessionTargetsFromEvents(events);
  if (!sessions.length) {
    return null;
  }
  const canOpen = localRuntimeBridgeAvailable();
  return (
    <div className="mb-4 rounded-lg border border-blue-500/30 bg-blue-500/5 p-3">
      <div className="mb-2 text-sm font-medium">{t('browserSession.activeSessions')}</div>
      <div className="space-y-2">
        {sessions.map((session) => (
          <div key={session.id} className="flex flex-wrap items-center justify-between gap-2 rounded-md border bg-background px-3 py-2">
            <div className="min-w-0">
              <div className="truncate text-xs font-medium">{session.title || session.id}</div>
              <div className="truncate text-[11px] text-muted-foreground">
                {session.id} · {session.workspaceId} · {session.status || 'active'}
              </div>
            </div>
            {canOpen ? (
              <button
                type="button"
                onClick={() => openBrowserSessionPanel(session)}
                className="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90"
              >
                {t('browserSession.openPanel')}
              </button>
            ) : (
              <span className="text-[11px] text-muted-foreground">
                {t('browserSession.desktopRequired')}
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
};
