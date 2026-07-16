// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { MessageList } from '../MessageList';
import type { ChatInterfaceProps, Message } from '../../types';
import { useI18n } from '../../i18n/I18nProvider';
import { LocalMemoryPolicyControls } from './LocalMemoryPolicyControls';
import { MemoryTimelineList, type MemoryTimelineItem } from './MemoryTimelineList';

interface SessionSummaryViewItem {
  id: string;
  summaryText: string;
  status: string;
  level: number;
  createdAt: string;
  updatedAt: string;
}

interface AgentRecallViewItem {
  id: string;
  recallKey: string;
  recallText: string;
  level: number;
  confidence?: number | null;
  lastSeenAt?: string | null;
  updatedAt: string;
  subjectType?: string | null;
}

interface SummaryPaneProps {
  sessionId: string;
  sessionTitle: string;
  contactName: string;
  projectName: string;
  projectId: string | null;
  messages: Message[];
  hasMore: boolean;
  onLoadMore: () => void;
  customRenderer?: ChatInterfaceProps['customRenderer'];
  sessionSummaries: SessionSummaryViewItem[];
  agentRecalls: AgentRecallViewItem[];
  memoryLoading: boolean;
  memoryError: string | null;
  onRefresh: () => void;
  onClose: () => void;
}

const SummaryPane: React.FC<SummaryPaneProps> = ({
  sessionId,
  sessionTitle,
  contactName,
  projectName,
  projectId,
  messages,
  hasMore,
  onLoadMore,
  customRenderer,
  sessionSummaries,
  agentRecalls,
  memoryLoading,
  memoryError,
  onRefresh,
  onClose,
}) => {
  const { t } = useI18n();
  const toTimestamp = (value?: string | null): number => {
    const parsed = value ? new Date(value).getTime() : Number.NaN;
    return Number.isFinite(parsed) ? parsed : 0;
  };

  const memoryTimeline: MemoryTimelineItem[] = [
    ...sessionSummaries.map((summary) => {
      const time = summary.updatedAt || summary.createdAt;
      return {
        id: `summary:${summary.id}`,
        sourceId: summary.id,
        kind: 'session_summary' as const,
        text: summary.summaryText || t('memory.emptySummary'),
        time,
        timeTs: Math.max(toTimestamp(summary.updatedAt), toTimestamp(summary.createdAt)),
        sourceLabel: t('memory.sessionSummarySource', { level: summary.level }),
      };
    }),
    ...agentRecalls.map((recall) => ({
      id: `recall:${recall.id}`,
      sourceId: recall.id,
      kind: 'agent_recall' as const,
      text: recall.recallText || t('memory.emptyRecall'),
      time: recall.updatedAt || recall.lastSeenAt || '',
      timeTs: Math.max(toTimestamp(recall.updatedAt), toTimestamp(recall.lastSeenAt)),
      sourceLabel: recall.subjectType === 'project'
        ? t('memory.projectRecallSource', { level: recall.level })
        : t('memory.agentRecallSource', { level: recall.level }),
    })),
  ].sort((left, right) => right.timeTs - left.timeTs);

  return (
    <div className="h-full min-h-0 flex flex-col overflow-hidden">
    <div className="basis-[42%] min-h-[170px] bg-card/40 flex flex-col overflow-hidden">
      <div className="px-3 py-2 border-b border-border flex items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="text-sm font-medium truncate">{t('memory.title')}</div>
          <div className="text-[11px] text-muted-foreground truncate">
            {contactName || sessionTitle || t('memory.currentContact')}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            {projectName ? t('memory.projectName', { name: projectName }) : t('memory.projectNone')}
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <button
            type="button"
            className="px-2 py-1 text-xs rounded border border-border hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
            disabled={memoryLoading}
            onClick={onRefresh}
          >
            {memoryLoading ? t('common.refreshing') : t('common.refresh')}
          </button>
          <button
            type="button"
            className="px-2 py-1 text-xs rounded border border-border hover:bg-accent"
            onClick={onClose}
          >
            {t('common.close')}
          </button>
        </div>
      </div>
      <div
        className="flex-1 min-h-0 overflow-y-auto px-3 py-3 space-y-3"
        style={{ overscrollBehavior: 'contain' }}
      >
        {memoryError ? (
          <div className="text-xs text-destructive">{memoryError}</div>
        ) : null}

        <LocalMemoryPolicyControls sessionId={sessionId} />

        <div className="rounded-lg border border-border bg-background/80 p-3">
          <div className="text-xs font-semibold text-foreground">{t('memory.entries')}</div>
          <div className="mt-1 text-[11px] text-muted-foreground">
            {projectId ? `project_id: ${projectId}` : t('memory.projectIdNone')}
          </div>
          <MemoryTimelineList
            sessionId={sessionId}
            items={memoryTimeline}
            onRefresh={onRefresh}
          />
        </div>
      </div>
    </div>
    <div className="relative shrink-0 px-3 py-1.5 bg-card/20">
      <div className="h-[2px] rounded-full bg-gradient-to-r from-transparent via-sky-400/95 to-transparent shadow-[0_0_16px_rgba(56,189,248,0.95)]" />
      <div className="pointer-events-none absolute inset-x-0 top-0 h-full bg-gradient-to-b from-sky-400/10 via-transparent to-transparent" />
    </div>
    <div className="flex-1 min-h-0 overflow-hidden">
      <MessageList
        key={`messages-${sessionId || 'none'}-summary`}
        sessionId={sessionId}
        messages={messages}
        isLoading={false}
        isStreaming={false}
        isStopping={false}
        assistantContactName={contactName}
        hasMore={hasMore}
        onLoadMore={onLoadMore}
        customRenderer={customRenderer}
      />
    </div>
  </div>
  );
};

export default SummaryPane;
