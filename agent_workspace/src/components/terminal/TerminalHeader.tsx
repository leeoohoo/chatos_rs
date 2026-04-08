import React from 'react';

import { cn } from '../../lib/utils';

export type TerminalConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';
export type TerminalHistoryState = 'idle' | 'loading' | 'ready' | 'error';

interface TerminalHeaderProps {
  terminalTitle: string;
  terminalCwd: string;
  connectionState: TerminalConnectionState;
  terminalStatus: string;
  historyState: TerminalHistoryState;
  historyBusy: boolean;
  canLoadMoreHistory: boolean;
  onLoadMoreHistory: () => void;
  onReconnect: () => void;
}

const TerminalHeader: React.FC<TerminalHeaderProps> = ({
  terminalTitle,
  terminalCwd,
  connectionState,
  terminalStatus,
  historyState,
  historyBusy,
  canLoadMoreHistory,
  onLoadMoreHistory,
  onReconnect,
}) => (
  <div className="flex items-center justify-between border-b border-border px-4 py-2">
    <div className="min-w-0">
      <div className="text-sm font-medium text-foreground truncate">{terminalTitle}</div>
      <div className="text-xs text-muted-foreground truncate">{terminalCwd}</div>
    </div>
    <div className="flex items-center gap-3 text-xs text-muted-foreground">
      <span className={cn(
        'inline-flex items-center gap-1',
        connectionState === 'connected' ? 'text-emerald-500' : connectionState === 'error' ? 'text-destructive' : 'text-muted-foreground'
      )}>
        <span className={cn(
          'inline-block h-2 w-2 rounded-full',
          connectionState === 'connected' ? 'bg-emerald-500' : connectionState === 'error' ? 'bg-destructive' : 'bg-muted-foreground/50'
        )} />
        {connectionState === 'connected' ? '已连接' : connectionState === 'connecting' ? '连接中' : connectionState === 'error' ? '连接错误' : '未连接'}
      </span>
      <span>状态: {terminalStatus}</span>
      <button
        type="button"
        disabled={historyState === 'loading' || historyBusy || !canLoadMoreHistory}
        onClick={onLoadMoreHistory}
        className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
      >
        {historyBusy ? '加载中...' : 'Load More History'}
      </button>
      <button
        type="button"
        onClick={onReconnect}
        className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
      >
        重连
      </button>
    </div>
  </div>
);

export default TerminalHeader;
