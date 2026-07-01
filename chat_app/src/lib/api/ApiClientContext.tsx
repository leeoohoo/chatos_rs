// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import ApiClient, { apiClient as globalApiClient } from './client';

const ApiClientContext = React.createContext<ApiClient>(globalApiClient);
const ApiClientPresenceContext = React.createContext(false);

interface ApiClientProviderProps {
  children: React.ReactNode;
  client?: ApiClient;
}

export const ApiClientProvider: React.FC<ApiClientProviderProps> = ({
  children,
  client,
}) => {
  const value = React.useMemo(() => client || globalApiClient, [client]);
  return (
    <ApiClientPresenceContext.Provider value>
      <ApiClientContext.Provider value={value}>
        {children}
      </ApiClientContext.Provider>
    </ApiClientPresenceContext.Provider>
  );
};

export const useApiClient = (): ApiClient => React.useContext(ApiClientContext);

export const useApiClientContext = (): ApiClient => {
  const client = React.useContext(ApiClientContext);
  const hasProvider = React.useContext(ApiClientPresenceContext);
  if (!hasProvider) {
    throw new Error('useApiClientContext must be used within an ApiClientProvider');
  }
  return client;
};

export const useOptionalApiClient = (): ApiClient | null => {
  const client = React.useContext(ApiClientContext);
  const hasProvider = React.useContext(ApiClientPresenceContext);
  return hasProvider ? client : null;
};
