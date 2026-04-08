import React from 'react';

import { formatCommandTime, type CommandHistoryItem } from './commandHistory';
import { renderHighlightedCommand } from './historyViewUtils';

interface TerminalCommandHistoryPanelProps {
  commandHistoryCount: number;
  displayHistory: CommandHistoryItem[];
}

const TerminalCommandHistoryPanel: React.FC<TerminalCommandHistoryPanelProps> = ({
  commandHistoryCount,
  displayHistory,
}) => (
  <div className="w-80 max-w-[45%] shrink-0 border-l border-border bg-card/40">
    <div className="border-b border-border px-3 py-2">
      <div className="text-sm font-medium text-foreground">历史命令</div>
      <div className="text-xs text-muted-foreground">{commandHistoryCount} 条（仅当前终端）</div>
    </div>

    <div className="h-[calc(100%-53px)] overflow-y-auto p-2">
      {displayHistory.length === 0 ? (
        <div className="rounded border border-dashed border-border px-3 py-4 text-xs text-muted-foreground">
          暂无命令，执行后会显示在这里
        </div>
      ) : (
        <div className="space-y-2">
          {displayHistory.map((item) => (
            <div key={item.id} className="rounded border border-border/60 bg-background/80 px-2 py-1.5">
              <div className="text-[10px] text-muted-foreground">{formatCommandTime(item.createdAt)}</div>
              <div className="mt-1 break-all font-mono text-xs whitespace-pre-wrap">
                {renderHighlightedCommand(item.command)}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  </div>
);

export default TerminalCommandHistoryPanel;
