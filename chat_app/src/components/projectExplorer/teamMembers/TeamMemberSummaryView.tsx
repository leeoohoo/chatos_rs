import React from 'react';

import { MessageList } from '../../MessageList';
import type { SessionSummaryItem } from '../../../features/sessionSummary/useSessionSummaryPanel';

interface TeamMemberSummaryViewProps {
  sessionId: string;
  sessionTitle: string;
  contactName: string;
  summaryItems: SessionSummaryItem[];
  summaryLoading: boolean;
  summaryError: string | null;
  clearingSummaries: boolean;
  deletingSummaryId: string | null;
  messages: any[];
  hasMoreMessages: boolean;
  chatIsLoading: boolean;
  chatIsStreaming: boolean;
  chatIsStopping: boolean;
  onLoadMore: () => void;
  onToggleTurnProcess: (userMessageId: string) => void;
  onClearSummaries: () => void;
  onRefreshSummaries: () => void;
  onCloseSummary: () => void;
  onDeleteSummary: (summaryId: string) => void;
}

const formatSummaryTime = (value?: string | null): string => {
  if (!value) {
    return '-';
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return parsed.toLocaleString();
};

const TeamMemberSummaryView: React.FC<TeamMemberSummaryViewProps> = ({
  sessionId,
  sessionTitle,
  contactName,
  summaryItems,
  summaryLoading,
  summaryError,
  clearingSummaries,
  deletingSummaryId,
  messages,
  hasMoreMessages,
  chatIsLoading,
  chatIsStreaming,
  chatIsStopping,
  onLoadMore,
  onToggleTurnProcess,
  onClearSummaries,
  onRefreshSummaries,
  onCloseSummary,
  onDeleteSummary,
}) => {
  return (
    <div className="h-full min-h-0 flex flex-col overflow-hidden">
      <div className="basis-[42%] min-h-[170px] bg-card/40 flex flex-col overflow-hidden border-b border-border">
        <div className="px-3 py-2 border-b border-border flex items-center justify-between gap-2">
          <div className="min-w-0">
            <div className="text-sm font-medium truncate">会话总结</div>
            <div className="text-[11px] text-muted-foreground truncate">
              {contactName || sessionTitle || '当前联系人'}
            </div>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <button
              type="button"
              className="px-2 py-1 text-xs rounded border border-border hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
              disabled={clearingSummaries || summaryLoading}
              onClick={onClearSummaries}
            >
              {clearingSummaries ? '清空中...' : '清空所有总结'}
            </button>
            <button
              type="button"
              className="px-2 py-1 text-xs rounded border border-border hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
              disabled={summaryLoading}
              onClick={onRefreshSummaries}
            >
              {summaryLoading ? '刷新中...' : '刷新'}
            </button>
            <button
              type="button"
              className="px-2 py-1 text-xs rounded border border-border hover:bg-accent"
              onClick={onCloseSummary}
            >
              关闭
            </button>
          </div>
        </div>
        <div className="flex-1 min-h-0 overflow-y-auto px-3 py-2 space-y-2">
          {summaryError ? (
            <div className="text-xs text-destructive">{summaryError}</div>
          ) : null}
          {summaryLoading ? (
            <div className="text-xs text-muted-foreground">正在加载会话总结...</div>
          ) : summaryItems.length === 0 ? (
            <div className="text-xs text-muted-foreground">当前会话暂无总结。</div>
          ) : (
            summaryItems.map((item) => (
              <div key={item.id} className="rounded-md border border-border bg-background/80 p-2">
                <div className="flex items-center justify-between gap-2">
                  <div className="min-w-0 text-[12px] text-muted-foreground truncate">
                    {item.triggerType || 'summary'}
                    {item.level > 0 ? ` · level ${item.level}` : ''}
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    <div className="text-[11px] text-muted-foreground">
                      {formatSummaryTime(item.createdAt)}
                    </div>
                    <button
                      type="button"
                      className="px-1.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground hover:text-destructive hover:border-destructive disabled:opacity-60"
                      onClick={() => onDeleteSummary(item.id)}
                      disabled={deletingSummaryId === item.id}
                    >
                      {deletingSummaryId === item.id ? '删除中' : '删除'}
                    </button>
                  </div>
                </div>
                <div className="mt-1 text-[11px] text-muted-foreground">
                  {`消息 ${item.sourceMessageCount} · 估算 ${item.sourceEstimatedTokens} tok`}
                </div>
                {item.status && item.status !== 'summarized' && (
                  <div className="mt-1 text-[11px] text-amber-600">
                    {item.status}
                  </div>
                )}
                {item.errorMessage && (
                  <div className="mt-1 text-[11px] text-destructive">
                    {item.errorMessage}
                  </div>
                )}
                <div className="mt-2 text-sm leading-6 whitespace-pre-wrap break-words">
                  {item.summaryText || '(空总结)'}
                </div>
              </div>
            ))
          )}
        </div>
      </div>
      <div className="relative shrink-0 px-3 py-1.5 bg-card/20">
        <div className="h-[2px] rounded-full bg-gradient-to-r from-transparent via-sky-400/95 to-transparent shadow-[0_0_16px_rgba(56,189,248,0.95)]" />
        <div className="pointer-events-none absolute inset-x-0 top-0 h-full bg-gradient-to-b from-sky-400/10 via-transparent to-transparent" />
      </div>
      <div className="flex-1 min-h-0 overflow-hidden">
        <MessageList
          key={`project-team-messages-${sessionId}-summary`}
          sessionId={sessionId}
          messages={messages}
          isLoading={chatIsLoading}
          isStreaming={chatIsStreaming}
          isStopping={chatIsStopping}
          hasMore={hasMoreMessages}
          onLoadMore={onLoadMore}
          onToggleTurnProcess={onToggleTurnProcess}
        />
      </div>
    </div>
  );
};

export default TeamMemberSummaryView;
