// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { getUserVisiblePath } from '../../lib/domain/filesystem';
import { cn } from '../../lib/utils';
import { useI18n } from '../../i18n/I18nProvider';

export type TerminalConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';
export type TerminalHistoryState = 'idle' | 'loading' | 'ready' | 'error';

interface TerminalHeaderProps {
  terminalTitle: string;
  terminalCwd: string;
  terminalDisplayCwd?: string | null;
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
  terminalDisplayCwd,
  connectionState,
  terminalStatus,
  historyState,
  historyBusy,
  canLoadMoreHistory,
  onLoadMoreHistory,
  onReconnect,
}) => {
  const { t } = useI18n();
  const localizedStatus = terminalStatus === 'running'
    ? t('terminal.status.running')
    : terminalStatus === 'exited'
      ? t('terminal.status.exited')
      : terminalStatus === 'unknown'
        ? t('common.unknown')
        : terminalStatus;
  const connectionLabel = connectionState === 'connected'
    ? t('terminal.connection.connected')
    : connectionState === 'connecting'
      ? t('terminal.connection.connecting')
      : connectionState === 'error'
        ? t('terminal.connection.error')
        : t('terminal.connection.disconnected');
  const visibleTerminalCwd = terminalDisplayCwd || getUserVisiblePath(terminalCwd);

  return (
    <div className="flex items-center justify-between border-b border-border px-4 py-2">
      <div className="min-w-0">
        <div className="text-sm font-medium text-foreground truncate">{terminalTitle}</div>
        <div className="text-xs text-muted-foreground truncate" title={visibleTerminalCwd}>
          {visibleTerminalCwd}
        </div>
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
          {connectionLabel}
        </span>
        <span>{t('terminal.statusLabel', { status: localizedStatus })}</span>
        <button
          type="button"
          disabled={historyState === 'loading' || historyBusy || !canLoadMoreHistory}
          onClick={onLoadMoreHistory}
          className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          {historyBusy ? t('common.loading') : t('terminal.history.loadMore')}
        </button>
        <button
          type="button"
          onClick={onReconnect}
          className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
        >
          {t('terminal.reconnect')}
        </button>
      </div>
    </div>
  );
};

export default TerminalHeader;
