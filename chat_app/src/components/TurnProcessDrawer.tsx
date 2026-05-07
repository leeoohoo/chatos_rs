import type { FC } from 'react';
import { MessageItem } from './MessageItem';
import { cn } from '../lib/utils';
import type { Message } from '../types';
import { useResizableTurnProcessPanel } from './turnProcessDrawer/useResizableTurnProcessPanel';
import { useTurnProcessDrawerModel } from './turnProcessDrawer/useTurnProcessDrawerModel';

interface TurnProcessDrawerProps {
  open: boolean;
  userMessageId: string | null;
  messages: Message[];
  isLoading?: boolean;
  onClose: () => void;
}

export const TurnProcessDrawer: FC<TurnProcessDrawerProps> = ({
  open,
  userMessageId,
  messages,
  isLoading = false,
  onClose,
}) => {
  const panelOpen = Boolean(open && userMessageId);
  const { panelWidth, handleResizeStart } = useResizableTurnProcessPanel(panelOpen);
  const {
    userMessage,
    assistantProcessMessages,
    toolResultById,
    assistantToolCallsById,
    assistantUnavailableTools,
    historyToolCount,
    historyThinkingCount,
    historyUnavailableCount,
  } = useTurnProcessDrawerModel({
    panelOpen,
    userMessageId,
    messages,
  });

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
                  Tools: {historyToolCount} · Thinking: {historyThinkingCount} · Unavailable: {historyUnavailableCount}
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

              {!isLoading && userMessage && assistantProcessMessages.length === 0 && assistantUnavailableTools.length === 0 && (
                <div className="text-sm text-muted-foreground">当前轮次暂无可展示的过程内容。</div>
              )}

              {assistantUnavailableTools.length > 0 && (
                <div className="rounded-md border border-amber-200 bg-amber-50 px-3 py-2 dark:border-amber-800/60 dark:bg-amber-950/30">
                  <div className="text-xs font-medium text-amber-900 dark:text-amber-200">
                    Unavailable tools ({assistantUnavailableTools.length})
                  </div>
                  <div className="mt-2 space-y-2">
                    {assistantUnavailableTools.map((item) => (
                      <div
                        key={item.id}
                        className="rounded border border-amber-200/80 bg-white/60 px-2 py-1.5 text-xs dark:border-amber-800/60 dark:bg-black/20"
                      >
                        <div className="font-medium text-amber-900 dark:text-amber-200">
                          @{item.serverName}_{item.toolName}
                        </div>
                        <div className="mt-0.5 text-amber-800 dark:text-amber-300">
                          {item.reason}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {assistantProcessMessages.map((message) => (
                <MessageItem
                  key={message.id}
                  message={message}
                  isStreaming={false}
                  renderContext="process_drawer"
                  toolResultById={toolResultById}
                  assistantToolCallsById={assistantToolCallsById}
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
