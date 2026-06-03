import React from 'react';

import { useApiClient } from '../lib/api/ApiClientContext';
import { useAuthStoreSelector } from '../lib/auth/authStore';
import { useChatStoreSelector } from '../lib/store/ChatStoreContext';
import { useTheme } from '../hooks/useTheme';
import EmbeddedTerminalView from './terminal/EmbeddedTerminalView';

interface TerminalViewProps {
  className?: string;
}

export const TerminalView: React.FC<TerminalViewProps> = ({ className }) => {
  const currentTerminal = useChatStoreSelector((state) => state.currentTerminal);
  const loadTerminals = useChatStoreSelector((state) => state.loadTerminals);
  const client = useApiClient();
  const { actualTheme } = useTheme();
  const accessToken = useAuthStoreSelector((state) => state.accessToken);

  return (
    <EmbeddedTerminalView
      terminal={currentTerminal}
      className={className}
      emptyText="请选择一个终端"
      loadTerminals={loadTerminals}
      client={client}
      accessToken={accessToken}
      actualTheme={actualTheme}
    />
  );
};

export default TerminalView;
