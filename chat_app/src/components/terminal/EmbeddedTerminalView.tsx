// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import '@xterm/xterm/css/xterm.css';

import { cn } from '../../lib/utils';
import type { Terminal } from '../../types';
import TerminalCommandHistoryPanel from './TerminalCommandHistoryPanel';
import TerminalHeader from './TerminalHeader';
import TerminalStatusBanners from './TerminalStatusBanners';
import { useTerminalRuntime } from './useTerminalRuntime';
import { useI18n } from '../../i18n/I18nProvider';

const NOOP_LOAD_TERMINALS = () => undefined;

interface EmbeddedTerminalViewProps {
  terminal: Terminal | null;
  className?: string;
  emptyText?: string;
  loadTerminals?: () => void | Promise<unknown>;
  client: {
    getBaseUrl(): string;
    issueWebSocketTicket(): Promise<string>;
    listTerminalLogs(
      terminalId: string,
      params?: { limit?: number; offset?: number; before?: string },
    ): Promise<import('../../lib/api/client/types').TerminalLogResponse[]>;
  };
  accessToken?: string | null;
  actualTheme: 'light' | 'dark';
}

export const EmbeddedTerminalView: React.FC<EmbeddedTerminalViewProps> = ({
  terminal,
  className,
  emptyText,
  loadTerminals = NOOP_LOAD_TERMINALS,
  client,
  accessToken,
  actualTheme,
}) => {
  const { t } = useI18n();
  const {
    containerRef,
    connectionState,
    historyState,
    historyBusy,
    canLoadMoreHistory,
    historyModeHint,
    errorMessage,
    commandHistoryCount,
    displayHistory,
    reconnect,
    loadMoreHistory,
  } = useTerminalRuntime({
    currentTerminal: terminal,
    loadTerminals,
    client,
    accessToken,
    actualTheme,
  });

  if (!terminal) {
    return (
      <div className={cn('flex h-full items-center justify-center text-sm text-muted-foreground', className)}>
        {emptyText || t('terminal.empty.dedicated')}
      </div>
    );
  }

  return (
    <div className={cn('flex h-full flex-col bg-card', className)}>
      <TerminalHeader
        terminalTitle={terminal.name || t('terminal.titleFallback')}
        terminalCwd={terminal.cwd || ''}
        terminalDisplayCwd={terminal.displayCwd || null}
        connectionState={connectionState}
        terminalStatus={terminal.status || 'unknown'}
        historyState={historyState}
        historyBusy={historyBusy}
        canLoadMoreHistory={canLoadMoreHistory}
        onLoadMoreHistory={loadMoreHistory}
        onReconnect={reconnect}
      />

      <TerminalStatusBanners
        historyState={historyState}
        historyModeHint={historyModeHint}
        errorMessage={errorMessage}
      />

      <div className="flex flex-1 overflow-hidden bg-background">
        <div className="min-w-0 flex-1 overflow-hidden">
          <div ref={containerRef} className="h-full w-full" />
        </div>

        <TerminalCommandHistoryPanel
          commandHistoryCount={commandHistoryCount}
          displayHistory={displayHistory}
        />
      </div>
    </div>
  );
};

export default EmbeddedTerminalView;
