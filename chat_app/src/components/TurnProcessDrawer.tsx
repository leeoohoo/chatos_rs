import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { MessageItem } from './MessageItem';
import { cn } from '../lib/utils';
import type { Message } from '../types';

interface TurnProcessDrawerProps {
  open: boolean;
  userMessageId: string | null;
  messages: Message[];
  isLoading?: boolean;
  onClose: () => void;
}

const MIN_PANEL_WIDTH = 360;
const DEFAULT_PANEL_WIDTH = 460;
const MAX_PANEL_WIDTH = 960;

const getMaxPanelWidth = (): number => {
  if (typeof window === 'undefined') {
    return MAX_PANEL_WIDTH;
  }
  return Math.max(MIN_PANEL_WIDTH, Math.min(MAX_PANEL_WIDTH, Math.floor(window.innerWidth * 0.75)));
};

const clampPanelWidth = (width: number, maxWidth: number = getMaxPanelWidth()): number => (
  Math.max(MIN_PANEL_WIDTH, Math.min(maxWidth, width))
);

const buildFallbackProcessMessage = (
  finalAssistantMessage: Message | null,
  userMessageId: string,
): Message | null => {
  if (!finalAssistantMessage || finalAssistantMessage.role !== 'assistant') {
    return null;
  }

  const metadata = finalAssistantMessage.metadata || {};
  const toolCalls = Array.isArray(metadata.toolCalls) ? metadata.toolCalls : [];
  const segments = Array.isArray(metadata.contentSegments) ? metadata.contentSegments : [];
  const processSegments = segments.filter((segment: any) => (
    segment?.type === 'thinking' || segment?.type === 'tool_call'
  ));

  const hasProcessContent = processSegments.length > 0 || toolCalls.length > 0;
  if (!hasProcessContent) {
    return null;
  }

  const normalizedSegments = processSegments.length > 0
    ? processSegments
    : toolCalls
      .filter((toolCall: any) => Boolean(toolCall?.id))
      .map((toolCall: any) => ({
        type: 'tool_call' as const,
        toolCallId: String(toolCall.id),
        content: '',
      }));

  if (normalizedSegments.length === 0) {
    return null;
  }

  return {
    ...finalAssistantMessage,
    id: `${finalAssistantMessage.id}__process_fallback`,
    content: '',
    metadata: {
      ...metadata,
      contentSegments: normalizedSegments,
      historyProcessUserMessageId: userMessageId,
      historyProcessLoaded: true,
      historyProcessPlaceholder: false,
    },
  };
};

export const TurnProcessDrawer: React.FC<TurnProcessDrawerProps> = ({
  open,
  userMessageId,
  messages,
  isLoading = false,
  onClose,
}) => {
  const panelOpen = Boolean(open && userMessageId);
  const [panelWidth, setPanelWidth] = useState<number>(DEFAULT_PANEL_WIDTH);

  useEffect(() => {
    const maxWidth = getMaxPanelWidth();
    setPanelWidth((current) => clampPanelWidth(current, maxWidth));
  }, [panelOpen]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }

    const onResize = () => {
      setPanelWidth((current) => clampPanelWidth(current));
    };

    window.addEventListener('resize', onResize);
    return () => {
      window.removeEventListener('resize', onResize);
    };
  }, []);

  useEffect(() => () => {
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  }, []);

  const handleResizeStart = useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    if (!panelOpen) {
      return;
    }

    event.preventDefault();

    const startX = event.clientX;
    const startWidth = panelWidth;
    const maxWidth = getMaxPanelWidth();

    const onMouseMove = (moveEvent: MouseEvent) => {
      const delta = startX - moveEvent.clientX;
      setPanelWidth(clampPanelWidth(startWidth + delta, maxWidth));
    };

    const stopResize = () => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', stopResize);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', stopResize);
  }, [panelOpen, panelWidth]);

  const userMessage = useMemo(() => {
    if (!panelOpen || !userMessageId) {
      return null;
    }
    return messages.find((message) => message.id === userMessageId && message.role === 'user') || null;
  }, [messages, panelOpen, userMessageId]);

  const processMessages = useMemo(() => {
    if (!panelOpen || !userMessageId) {
      return [] as Message[];
    }

    return messages.filter((message) => (
      message.metadata?.historyProcessUserMessageId === userMessageId
      && message.metadata?.historyProcessPlaceholder !== true
    ));
  }, [messages, panelOpen, userMessageId]);

  const finalAssistantMessage = useMemo(() => {
    if (!panelOpen || !userMessageId) {
      return null;
    }

    return messages.find((message) => (
      message.role === 'assistant'
      && message.metadata?.historyFinalForUserMessageId === userMessageId
    )) || null;
  }, [messages, panelOpen, userMessageId]);

  const fallbackProcessMessage = useMemo(() => {
    if (!panelOpen || !userMessageId || processMessages.length > 0) {
      return null;
    }
    return buildFallbackProcessMessage(finalAssistantMessage, userMessageId);
  }, [finalAssistantMessage, panelOpen, processMessages.length, userMessageId]);

  const assistantProcessMessages = useMemo(() => {
    const base = processMessages.filter((message) => message.role === 'assistant');
    if (base.length > 0) {
      return base;
    }
    if (fallbackProcessMessage) {
      return [fallbackProcessMessage];
    }
    return [] as Message[];
  }, [fallbackProcessMessage, processMessages]);

  const toolResultById = useMemo(() => {
    const map = new Map<string, Message>();
    for (const message of messages || []) {
      if (message.role !== 'tool') continue;
      const raw = message as any;
      const toolCallId = raw.tool_call_id || raw.toolCallId || message.metadata?.tool_call_id || message.metadata?.toolCallId;
      if (toolCallId) {
        map.set(String(toolCallId), message);
      }
    }
    return map;
  }, [messages]);

  const historyProcess = userMessage?.metadata?.historyProcess as any;
  const historyToolCount = Number(historyProcess?.toolCallCount || 0);
  const historyThinkingCount = Number(historyProcess?.thinkingCount || 0);

  return (
    <aside
      className={cn(
        'relative h-full min-h-0 bg-card transition-[width] duration-200 overflow-hidden flex flex-col',
        panelOpen ? 'border-l border-border' : 'border-l-0',
      )}
      style={{ width: panelOpen ? `${panelWidth}px` : '0px' }}
    >
      {panelOpen && (
        <>
          <div
            className="absolute inset-y-0 left-0 z-20 w-1.5 cursor-col-resize hover:bg-border/80 active:bg-primary/30"
            onMouseDown={handleResizeStart}
            title="拖动调整宽度"
          />

          <div className="flex-1 min-h-0 min-w-0 flex flex-col">
            <div className="flex items-center justify-between px-3 py-2 border-b border-border">
              <div className="min-w-0">
                <h2 className="text-sm font-semibold text-foreground truncate">过程详情</h2>
                <p className="text-xs text-muted-foreground mt-0.5 truncate">
                  Tools: {historyToolCount} · Thinking: {historyThinkingCount}
                </p>
              </div>

              <button
                type="button"
                onClick={onClose}
                className="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent"
                title="收起过程面板"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 18l-6-6 6-6" />
                </svg>
              </button>
            </div>

            <div className="flex-1 min-h-0 overflow-y-auto p-3 space-y-3">
              {isLoading && assistantProcessMessages.length === 0 && (
                <div className="text-sm text-muted-foreground">Loading process...</div>
              )}

              {!isLoading && !userMessage && (
                <div className="text-sm text-muted-foreground">未找到对应的用户消息。</div>
              )}

              {!isLoading && userMessage && assistantProcessMessages.length === 0 && (
                <div className="text-sm text-muted-foreground">当前轮次暂无可展示的过程内容。</div>
              )}

              {assistantProcessMessages.map((message) => (
                <MessageItem
                  key={message.id}
                  message={message}
                  isStreaming={false}
                  renderContext="process_drawer"
                  allMessages={messages}
                  toolResultById={toolResultById}
                />
              ))}
            </div>
          </div>
        </>
      )}
    </aside>
  );
};

export default TurnProcessDrawer;
