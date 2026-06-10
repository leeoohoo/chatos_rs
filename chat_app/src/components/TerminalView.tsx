import React from 'react';

import { useApiClient } from '../lib/api/ApiClientContext';
import { useAuthStoreSelector } from '../lib/auth/authStore';
import { useChatStoreSelector } from '../lib/store/ChatStoreContext';
import { useTheme } from '../hooks/useTheme';
import EmbeddedTerminalView from './terminal/EmbeddedTerminalView';
import { useI18n } from '../i18n/I18nProvider';

interface TerminalViewProps {
  className?: string;
}

export const TerminalView: React.FC<TerminalViewProps> = ({ className }) => {
  const currentTerminal = useChatStoreSelector((state) => state.currentTerminal);
  const loadTerminals = useChatStoreSelector((state) => state.loadTerminals);
  const client = useApiClient();
  const { actualTheme } = useTheme();
  const accessToken = useAuthStoreSelector((state) => state.accessToken);
  const { t } = useI18n();

  return (
    <EmbeddedTerminalView
      terminal={currentTerminal}
      className={className}
      emptyText={t('terminal.empty.select')}
      loadTerminals={loadTerminals}
      client={client}
      accessToken={accessToken}
      actualTheme={actualTheme}
    />
  );
};

export default TerminalView;
