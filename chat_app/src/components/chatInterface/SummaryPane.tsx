import React from 'react';
import { MessageList } from '../MessageList';
import { MarkdownRenderer } from '../MarkdownRenderer';
import type { ChatInterfaceProps, Message } from '../../types';

interface ProjectMemoryViewItem {
  id: string;
  projectId: string;
  memoryText: string;
  memoryVersion: number;
  lastSourceAt?: string | null;
  updatedAt: string;
}

interface AgentRecallViewItem {
  id: string;
  recallKey: string;
  recallText: string;
  sourceProjectIds: string[];
  confidence?: number | null;
  lastSeenAt?: string | null;
  updatedAt: string;
}

interface SummaryPaneProps {
  sessionId: string;
  sessionTitle: string;
  contactName: string;
  projectName: string;
  projectId: string | null;
  messages: Message[];
  isLoading: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
  onToggleTurnProcess: (userMessageId: string) => void;
  customRenderer?: ChatInterfaceProps['customRenderer'];
  projectMemories: ProjectMemoryViewItem[];
  agentRecalls: AgentRecallViewItem[];
  memoryLoading: boolean;
  memoryError: string | null;
  onRefresh: () => void;
  onClose: () => void;
}

const formatTextDate = (value?: string | null): string => {
  if (!value) {
    return '-';
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return parsed.toLocaleString();
};

const SummaryPane: React.FC<SummaryPaneProps> = ({
  sessionId,
  sessionTitle,
  contactName,
  projectName,
  projectId,
  messages,
  isLoading,
  isStreaming,
  isStopping,
  hasMore,
  onLoadMore,
  onToggleTurnProcess,
  customRenderer,
  projectMemories,
  agentRecalls,
  memoryLoading,
  memoryError,
  onRefresh,
  onClose,
}) => (
  <div className="h-full min-h-0 flex flex-col overflow-hidden">
    <div className="basis-[42%] min-h-[170px] bg-card/40 flex flex-col overflow-hidden">
      <div className="px-3 py-2 border-b border-border flex items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="text-sm font-medium truncate">记忆视图</div>
          <div className="text-[11px] text-muted-foreground truncate">
            {contactName || sessionTitle || '当前联系人'}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            {projectName ? `项目：${projectName}` : '项目：未选择'}
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <button
            type="button"
            className="px-2 py-1 text-xs rounded border border-border hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
            disabled={memoryLoading}
            onClick={onRefresh}
          >
            {memoryLoading ? '刷新中...' : '刷新'}
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
        {memoryError ? (
          <div className="text-xs text-destructive">{memoryError}</div>
        ) : null}

        <div className="rounded-lg border border-border bg-background/80 p-3">
          <div className="text-xs font-semibold text-foreground">项目记忆</div>
          <div className="mt-1 text-[11px] text-muted-foreground">
            {projectId ? `project_id: ${projectId}` : '当前未绑定项目'}
          </div>
          {projectMemories.length === 0 ? (
            <div className="mt-2 text-xs text-muted-foreground">
              当前项目暂无记忆。
            </div>
          ) : (
            <div className="mt-2 space-y-3">
              {projectMemories.map((memory) => (
                <div key={memory.id} className="rounded border border-border p-2">
                  <div className="text-[11px] text-muted-foreground">
                    {`版本 ${memory.memoryVersion} · 更新于 ${formatTextDate(memory.updatedAt)}`}
                  </div>
                  <div className="text-[11px] text-muted-foreground">
                    {`来源时间 ${formatTextDate(memory.lastSourceAt)}`}
                  </div>
                  <div className="mt-1 text-sm leading-6">
                    <MarkdownRenderer content={memory.memoryText || '(空记忆)'} />
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="rounded-lg border border-border bg-background/80 p-3">
          <div className="text-xs font-semibold text-foreground">智能体回忆</div>
          {agentRecalls.length === 0 ? (
            <div className="mt-2 text-xs text-muted-foreground">
              当前智能体暂无回忆。
            </div>
          ) : (
            <div className="mt-2 space-y-2">
              {agentRecalls.map((recall) => (
                <div key={recall.id} className="rounded border border-border p-2">
                  <div className="text-[11px] text-muted-foreground">
                    {recall.recallKey}
                  </div>
                  <div className="mt-1 text-sm leading-6">
                    <MarkdownRenderer content={recall.recallText || '(空回忆)'} />
                  </div>
                  <div className="mt-1 text-[11px] text-muted-foreground">
                    {`来源项目: ${recall.sourceProjectIds.length > 0 ? recall.sourceProjectIds.join(', ') : '-'}`}
                  </div>
                  <div className="text-[11px] text-muted-foreground">
                    {`置信度: ${typeof recall.confidence === 'number' ? recall.confidence.toFixed(2) : '-'} · 最近看到: ${formatTextDate(recall.lastSeenAt)} · 更新于 ${formatTextDate(recall.updatedAt)}`}
                  </div>
                </div>
              ))}
            </div>
          )}
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
        isLoading={isLoading}
        isStreaming={isStreaming}
        isStopping={isStopping}
        hasMore={hasMore}
        onLoadMore={onLoadMore}
        onToggleTurnProcess={onToggleTurnProcess}
        customRenderer={customRenderer}
      />
    </div>
  </div>
);

export default SummaryPane;
