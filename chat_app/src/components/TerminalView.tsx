import React from 'react';

import { useAuthStore } from '../lib/auth/authStore';
import { apiClient } from '../lib/api/client';
import { useChatApiClientFromContext, useChatStoreSelector } from '../lib/store/ChatStoreContext';
import { useTheme } from '../hooks/useTheme';
import EmbeddedTerminalView from './terminal/EmbeddedTerminalView';

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
