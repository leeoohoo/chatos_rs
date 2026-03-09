import React from 'react';
import { MessageList } from '../MessageList';
import { MarkdownRenderer } from '../MarkdownRenderer';
import type { SessionSummaryWorkbarItem } from '../TaskWorkbar';
import type { ChatInterfaceProps, Message } from '../../types';

interface SummaryPaneProps {
  sessionId: string;
  sessionTitle: string;
  messages: Message[];
  isLoading: boolean;
  isStreaming: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
  onToggleTurnProcess: (userMessageId: string) => void;
  customRenderer?: ChatInterfaceProps['customRenderer'];
  summaries: SessionSummaryWorkbarItem[];
  summariesLoading: boolean;
  summariesError: string | null;
  actionLoadingKey: string | null;
  onClearAll: () => void;
  onRefresh: () => void;
  onClose: () => void;
  onDeleteSummary: (summary: SessionSummaryWorkbarItem) => void;
  formatCreatedAt: (value: string) => string;
}

const SummaryPane: React.FC<SummaryPaneProps> = ({
  sessionId,
  sessionTitle,
  messages,
  isLoading,
  isStreaming,
  hasMore,
  onLoadMore,
  onToggleTurnProcess,
  customRenderer,
  summaries,
  summariesLoading,
  summariesError,
  actionLoadingKey,
  onClearAll,
  onRefresh,
  onClose,
  onDeleteSummary,
  formatCreatedAt,
}) => (
  <div className="h-full min-h-0 flex flex-col overflow-hidden">
    <div className="basis-[42%] min-h-[170px] bg-card/40 flex flex-col overflow-hidden">
      <div className="px-3 py-2 border-b border-border flex items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="text-sm font-medium truncate">会话总结</div>
          <div className="text-[11px] text-muted-foreground truncate">{sessionTitle}</div>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          {summaries.length > 0 ? (
            <button
              type="button"
              className="px-2 py-1 text-xs rounded border border-border hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
              disabled={actionLoadingKey !== null}
              onClick={onClearAll}
            >
              {actionLoadingKey === 'clear-all' ? '清空中...' : '清空所有总结'}
            </button>
          ) : null}
          <button
            type="button"
            className="px-2 py-1 text-xs rounded border border-border hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
            disabled={actionLoadingKey !== null || summariesLoading}
            onClick={onRefresh}
          >
            刷新
          </button>
          <button
            type="button"
            className="px-2 py-1 text-xs rounded border border-border hover:bg-accent"
            onClick={onClose}
          >
            关闭
          </button>
        </div>
      </div>
      <div
        className="flex-1 min-h-0 overflow-y-auto px-3 py-3 space-y-3"
        style={{ overscrollBehavior: 'contain' }}
      >
        {summariesLoading ? (
          <div className="text-xs text-muted-foreground">总结加载中...</div>
        ) : summariesError ? (
          <div className="text-xs text-destructive">{summariesError}</div>
        ) : summaries.length === 0 ? (
          <div className="text-xs text-muted-foreground">当前会话暂无总结。</div>
        ) : (
          summaries.map((summary) => (
            <div key={summary.id} className="rounded-lg border border-border bg-background/80 p-3">
              <div className="flex items-center justify-between gap-2 text-[11px] text-muted-foreground">
                <span className="truncate">{summary.triggerType || '-'}</span>
                <div className="flex items-center gap-2 shrink-0">
                  <span className="shrink-0">{formatCreatedAt(summary.createdAt)}</span>
                  <button
                    type="button"
                    className="rounded border border-border px-1.5 py-0.5 text-[10px] text-foreground hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
                    disabled={actionLoadingKey !== null}
                    onClick={() => {
                      onDeleteSummary(summary);
                    }}
                  >
                    {actionLoadingKey === `delete:${summary.id}` ? '删除中...' : '删除'}
                  </button>
                </div>
              </div>
              <div className="mt-1 text-[11px] text-muted-foreground">
                {`消息 ${summary.sourceMessageCount} · 估算 ${summary.sourceEstimatedTokens} tok`}
              </div>
              {summary.status && summary.status !== 'done' ? (
                <div className="mt-1 text-[11px] text-amber-600">
                  {summary.errorMessage || summary.status}
                </div>
              ) : null}
              <div className="mt-2 text-sm leading-6">
                <MarkdownRenderer content={summary.summaryText || '(空总结)'} />
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
        key={`messages-${sessionId || 'none'}-summary`}
        sessionId={sessionId}
        messages={messages}
        isLoading={isLoading}
        isStreaming={isStreaming}
        hasMore={hasMore}
        onLoadMore={onLoadMore}
        onToggleTurnProcess={onToggleTurnProcess}
        customRenderer={customRenderer}
      />
    </div>
  </div>
);

export default SummaryPane;
