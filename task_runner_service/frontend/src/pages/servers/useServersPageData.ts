// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';

import { api } from '../../api/client';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type { RemoteServerAuthType } from '../../types';
import {
  authTypeLabelKeys,
  type ServerEnabledFilter,
} from './serverPageUtils';

type UseServersPageDataParams = {
  t: TranslateFn;
  routeServerId?: string;
  keywordFilter: string;
  authTypeFilter: 'all' | RemoteServerAuthType;
  enabledFilter: ServerEnabledFilter;
};

export function useServersPageData({
  t,
  routeServerId,
  keywordFilter,
  authTypeFilter,
  enabledFilter,
}: UseServersPageDataParams) {
  const authTypeOptions = useMemo(
    () => (Object.keys(authTypeLabelKeys) as RemoteServerAuthType[]).map((value) => ({
      label: t(authTypeLabelKeys[value]),
      value,
    })),
    [t],
  );
  const authTypeFilterOptions = useMemo(
    () => [
      { label: t('servers.auth.all'), value: 'all' },
      ...authTypeOptions,
    ],
    [authTypeOptions, t],
  );
  const enabledFilterOptions = useMemo(
    () => [
      { label: t('servers.filter.all'), value: 'all' },
      { label: t('servers.filter.enabled'), value: 'enabled' },
      { label: t('servers.filter.disabled'), value: 'disabled' },
    ],
    [t],
  );

  const serversQuery = useQuery({
    queryKey: ['remote-servers'],
    queryFn: api.listRemoteServers,
  });
  const selectedServerQuery = useQuery({
    queryKey: ['remote-server', routeServerId],
    queryFn: () => api.getRemoteServer(routeServerId!),
    enabled: Boolean(routeServerId),
  });

  const selectedServer = useMemo(() => {
    if (!routeServerId) {
      return null;
    }
    return (
      selectedServerQuery.data ||
      (serversQuery.data || []).find((server) => server.id === routeServerId) ||
      null
    );
  }, [routeServerId, selectedServerQuery.data, serversQuery.data]);

  const filteredServers = useMemo(() => {
    const keyword = keywordFilter.trim().toLowerCase();
    return (serversQuery.data || []).filter((server) => {
      if (authTypeFilter !== 'all' && server.auth_type !== authTypeFilter) {
        return false;
      }
      if (enabledFilter === 'enabled' && !server.enabled) {
        return false;
      }
      if (enabledFilter === 'disabled' && server.enabled) {
        return false;
      }
      if (!keyword) {
        return true;
      }
      return [
        server.name,
        server.host,
        server.username,
        server.default_remote_path || '',
        server.last_test_message || '',
      ]
        .join(' ')
        .toLowerCase()
        .includes(keyword);
    });
  }, [authTypeFilter, enabledFilter, keywordFilter, serversQuery.data]);

  const stats = useMemo(
    () => ({
      visible: filteredServers.length,
      enabled: filteredServers.filter((server) => server.enabled).length,
      testPassed: filteredServers.filter((server) => server.last_test_status === 'success').length,
      strict: filteredServers.filter((server) => server.host_key_policy === 'strict').length,
    }),
    [filteredServers],
  );

  return {
    authTypeOptions,
    authTypeFilterOptions,
    enabledFilterOptions,
    serversQuery,
    selectedServerQuery,
    selectedServer,
    filteredServers,
    stats,
  };
}
