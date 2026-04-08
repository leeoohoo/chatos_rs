import React from 'react';

import type { TerminalHistoryState } from './TerminalHeader';

interface TerminalStatusBannersProps {
  historyState: TerminalHistoryState;
  historyModeHint: string | null;
  errorMessage: string | null;
}

const TerminalStatusBanners: React.FC<TerminalStatusBannersProps> = ({
  historyState,
  historyModeHint,
  errorMessage,
}) => (
  <>
    {historyState === 'loading' && (
      <div className="px-4 py-2 text-xs text-muted-foreground">加载历史记录中...</div>
    )}
    {historyModeHint && !errorMessage && (
      <div className="px-4 py-2 text-xs text-muted-foreground">{historyModeHint}</div>
    )}
    {errorMessage && (
      <div className="px-4 py-2 text-xs text-destructive">{errorMessage}</div>
    )}
  </>
);

export default TerminalStatusBanners;
