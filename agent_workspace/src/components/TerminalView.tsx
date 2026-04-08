import React from 'react';
import '@xterm/xterm/css/xterm.css';

import { useAuthStore } from '../lib/auth/authStore';
import { apiClient } from '../lib/api/client';
import { useChatApiClientFromContext, useChatStoreSelector } from '../lib/store/ChatStoreContext';
import { cn } from '../lib/utils';
import { useTheme } from '../hooks/useTheme';
import TerminalCommandHistoryPanel from './terminal/TerminalCommandHistoryPanel';
import TerminalHeader from './terminal/TerminalHeader';
import TerminalStatusBanners from './terminal/TerminalStatusBanners';
import { useTerminalRuntime } from './terminal/useTerminalRuntime';

interface TerminalViewProps {
  className?: string;
}

export const TerminalView: React.FC<TerminalViewProps> = ({ className }) => {
  const currentTerminal = useChatStoreSelector((state) => state.currentTerminal);
  const loadTerminals = useChatStoreSelector((state) => state.loadTerminals);
  const apiClientFromContext = useChatApiClientFromContext();
  const { actualTheme } = useTheme();

  const client = apiClientFromContext ?? apiClient;
  const accessToken = useAuthStore((state) => state.accessToken);

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
    currentTerminal,
    loadTerminals,
    client,
    accessToken,
    actualTheme,
  });

  if (!currentTerminal) {
    return (
      <div className={cn('flex h-full items-center justify-center text-muted-foreground', className)}>
        请选择一个终端
      </div>
    );
  }

  const terminalTitle = currentTerminal?.name || '终端';
  const terminalCwd = currentTerminal?.cwd || '';
  const terminalStatus = currentTerminal?.status || 'unknown';

  return (
    <div className={cn('flex h-full flex-col bg-card', className)}>
      <TerminalHeader
        terminalTitle={terminalTitle}
        terminalCwd={terminalCwd}
        connectionState={connectionState}
        terminalStatus={terminalStatus}
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

export default TerminalView;
