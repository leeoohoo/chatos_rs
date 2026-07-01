// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import type { TerminalHistoryState } from './TerminalHeader';
import { useI18n } from '../../i18n/I18nProvider';

interface TerminalStatusBannersProps {
  historyState: TerminalHistoryState;
  historyModeHint: string | null;
  errorMessage: string | null;
}

const TerminalStatusBanners: React.FC<TerminalStatusBannersProps> = ({
  historyState,
  historyModeHint,
  errorMessage,
}) => {
  const { t } = useI18n();
  const translatedHistoryModeHint = historyModeHint?.startsWith('terminal.')
    ? t(historyModeHint)
    : historyModeHint;

  return (
    <>
      {historyState === 'loading' && (
        <div className="px-4 py-2 text-xs text-muted-foreground">{t('terminal.history.loading')}</div>
      )}
      {translatedHistoryModeHint && !errorMessage && (
        <div className="px-4 py-2 text-xs text-muted-foreground">{translatedHistoryModeHint}</div>
      )}
      {errorMessage && (
        <div className="px-4 py-2 text-xs text-destructive">{errorMessage}</div>
      )}
    </>
  );
};

export default TerminalStatusBanners;
