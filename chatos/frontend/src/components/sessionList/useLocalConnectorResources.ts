// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import type {
  LocalConnectorDirectoryEntryResponse,
  LocalConnectorDeviceResponse,
  LocalConnectorWorkspaceResponse,
} from '../../lib/api/client/types';
import type {
  LocalConnectorDirectoryEntryOption,
  LocalConnectorWorkspaceOption,
} from './CreateResourceModals';

const valueAsString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const valueAsBoolean = (value: unknown): boolean => (
  typeof value === 'boolean' ? value : false
);

const normalizeLocalConnectorDirectoryPath = (value: unknown): string => {
  const normalized = valueAsString(value).replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
  return normalized && normalized !== '.' ? normalized : '.';
};

const joinLocalConnectorDirectoryPath = (parent: string, name: string): string => {
  const cleanParent = normalizeLocalConnectorDirectoryPath(parent);
  const cleanName = name.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
  return cleanParent === '.' ? cleanName : `${cleanParent}/${cleanName}`;
};

const localConnectorDeviceId = (device: LocalConnectorDeviceResponse): string => (
  valueAsString(device.id)
);

const localConnectorDeviceLabel = (device: LocalConnectorDeviceResponse): string => (
  valueAsString(device.display_name)
  || valueAsString(device.displayName)
  || valueAsString(device.os)
  || localConnectorDeviceId(device)
);

const localConnectorDeviceStatus = (device: LocalConnectorDeviceResponse): string => (
  valueAsString(device.status) || 'registered'
);

const localConnectorWorkspaceDeviceId = (workspace: LocalConnectorWorkspaceResponse): string => (
  valueAsString(workspace.device_id)
  || valueAsString(workspace.deviceId)
);

const localConnectorWorkspaceAlias = (workspace: LocalConnectorWorkspaceResponse): string => (
  valueAsString(workspace.local_path_alias)
  || valueAsString(workspace.localPathAlias)
  || valueAsString(workspace.display_name)
  || valueAsString(workspace.displayName)
  || valueAsString(workspace.id)
);

const localConnectorWorkspaceStatus = (workspace: LocalConnectorWorkspaceResponse): string => (
  valueAsString(workspace.status) || 'active'
);

const localConnectorDirectoryEntryName = (entry: LocalConnectorDirectoryEntryResponse): string => (
  valueAsString(entry.name) || valueAsString(entry.path)
);

const normalizeLocalConnectorDirectoryEntries = (
  entries: LocalConnectorDirectoryEntryResponse[] | undefined,
): LocalConnectorDirectoryEntryOption[] => (
  (Array.isArray(entries) ? entries : [])
    .map((entry) => {
      const name = localConnectorDirectoryEntryName(entry);
      const path = normalizeLocalConnectorDirectoryPath(entry.path);
      const isDir = valueAsBoolean(entry.is_dir) || valueAsBoolean(entry.isDir);
      return {
        name,
        path,
        isDir,
      };
    })
    .filter((entry) => entry.name && entry.path && entry.isDir)
);

const normalizeLocalConnectorWorkspaces = (
  devices: LocalConnectorDeviceResponse[],
  workspaces: LocalConnectorWorkspaceResponse[],
): LocalConnectorWorkspaceOption[] => {
  const deviceLabels = new Map<string, string>();
  const deviceStatuses = new Map<string, string>();
  devices.forEach((device) => {
    const id = localConnectorDeviceId(device);
    if (id) {
      deviceLabels.set(id, localConnectorDeviceLabel(device));
      deviceStatuses.set(id, localConnectorDeviceStatus(device));
    }
  });
  return workspaces
    .map((workspace) => {
      const id = valueAsString(workspace.id);
      const deviceId = localConnectorWorkspaceDeviceId(workspace);
      const alias = localConnectorWorkspaceAlias(workspace);
      const deviceLabel = deviceLabels.get(deviceId) || null;
      const deviceStatus = deviceStatuses.get(deviceId) || null;
      const label = [alias, deviceLabel].filter(Boolean).join(' · ');
      return {
        id,
        deviceId,
        alias,
        label: label || id,
        deviceLabel,
        deviceStatus,
        status: localConnectorWorkspaceStatus(workspace),
      };
    })
    .filter((workspace) => (
      workspace.id
      && workspace.deviceId
      && workspace.deviceStatus === 'online'
      && workspace.status !== 'disabled'
    ));
};

interface UseLocalConnectorResourcesParams {
  apiClient: ApiClient;
  t: (key: string) => string;
}

export const useLocalConnectorResources = ({
  apiClient,
  t,
}: UseLocalConnectorResourcesParams) => {
  const [localConnectorWorkspaces, setLocalConnectorWorkspaces] = useState<LocalConnectorWorkspaceOption[]>([]);
  const [localConnectorLoading, setLocalConnectorLoading] = useState(false);
  const [localConnectorError, setLocalConnectorError] = useState<string | null>(null);
  const [selectedLocalConnectorWorkspaceId, setSelectedLocalConnectorWorkspaceId] = useState('');
  const [localConnectorDirectoryPath, setLocalConnectorDirectoryPath] = useState('.');
  const [localConnectorDirectoryParent, setLocalConnectorDirectoryParent] = useState<string | null>(null);
  const [localConnectorDirectoryEntries, setLocalConnectorDirectoryEntries] = useState<LocalConnectorDirectoryEntryOption[]>([]);
  const [localConnectorDirectoryLoading, setLocalConnectorDirectoryLoading] = useState(false);
  const [localConnectorDirectoryError, setLocalConnectorDirectoryError] = useState<string | null>(null);
  const [selectedLocalConnectorDirectoryPath, setSelectedLocalConnectorDirectoryPath] = useState('');

  const loadLocalConnectorDirectoryForWorkspace = useCallback(async (
    workspace: LocalConnectorWorkspaceOption | null,
    path = '.',
  ) => {
    if (!workspace) {
      setLocalConnectorDirectoryPath('.');
      setLocalConnectorDirectoryParent(null);
      setLocalConnectorDirectoryEntries([]);
      setSelectedLocalConnectorDirectoryPath('');
      return;
    }
    const normalizedPath = normalizeLocalConnectorDirectoryPath(path);
    setLocalConnectorDirectoryLoading(true);
    setLocalConnectorDirectoryError(null);
    try {
      const result = await apiClient.listLocalConnectorDirectory({
        device_id: workspace.deviceId,
        workspace_id: workspace.id,
        path: normalizedPath,
      });
      const nextPath = normalizeLocalConnectorDirectoryPath(result.path);
      const parent = result.parent ? normalizeLocalConnectorDirectoryPath(result.parent) : null;
      setLocalConnectorDirectoryPath(nextPath);
      setLocalConnectorDirectoryParent(parent && parent !== nextPath ? parent : null);
      setLocalConnectorDirectoryEntries(normalizeLocalConnectorDirectoryEntries(result.entries));
      setSelectedLocalConnectorDirectoryPath(nextPath === '.' ? '' : nextPath);
    } catch (error) {
      setLocalConnectorDirectoryEntries([]);
      setLocalConnectorDirectoryError(error instanceof Error ? error.message : t('sessionList.resource.localConnectorDirectoryLoadFailed'));
    } finally {
      setLocalConnectorDirectoryLoading(false);
    }
  }, [apiClient, t]);

  const refreshLocalConnectorWorkspaces = useCallback(async () => {
    setLocalConnectorLoading(true);
    setLocalConnectorError(null);
    try {
      const [devices, workspaces] = await Promise.all([
        apiClient.listLocalConnectorDevices(),
        apiClient.listLocalConnectorWorkspaces(),
      ]);
      const normalized = normalizeLocalConnectorWorkspaces(
        Array.isArray(devices) ? devices : [],
        Array.isArray(workspaces) ? workspaces : [],
      );
      setLocalConnectorWorkspaces(normalized);
      const nextWorkspace = normalized.find((workspace) => workspace.id === selectedLocalConnectorWorkspaceId)
        || normalized[0]
        || null;
      setSelectedLocalConnectorWorkspaceId(nextWorkspace?.id || '');
      await loadLocalConnectorDirectoryForWorkspace(nextWorkspace, '.');
    } catch (error) {
      setLocalConnectorWorkspaces([]);
      setLocalConnectorDirectoryPath('.');
      setLocalConnectorDirectoryParent(null);
      setLocalConnectorDirectoryEntries([]);
      setSelectedLocalConnectorDirectoryPath('');
      setLocalConnectorError(error instanceof Error ? error.message : t('sessionList.resource.localConnectorLoadFailed'));
    } finally {
      setLocalConnectorLoading(false);
    }
  }, [apiClient, loadLocalConnectorDirectoryForWorkspace, selectedLocalConnectorWorkspaceId, t]);

  const handleSelectedLocalConnectorWorkspaceChange = useCallback((workspaceId: string) => {
    setSelectedLocalConnectorWorkspaceId(workspaceId);
    setSelectedLocalConnectorDirectoryPath('');
    const workspace = localConnectorWorkspaces.find((item) => item.id === workspaceId) || null;
    void loadLocalConnectorDirectoryForWorkspace(workspace, '.');
  }, [loadLocalConnectorDirectoryForWorkspace, localConnectorWorkspaces]);

  const browseLocalConnectorDirectory = useCallback((path: string) => {
    const workspace = localConnectorWorkspaces.find((item) => item.id === selectedLocalConnectorWorkspaceId) || null;
    void loadLocalConnectorDirectoryForWorkspace(workspace, path);
  }, [loadLocalConnectorDirectoryForWorkspace, localConnectorWorkspaces, selectedLocalConnectorWorkspaceId]);

  const createLocalConnectorDirectory = useCallback(async (name: string) => {
    const workspace = localConnectorWorkspaces.find((item) => item.id === selectedLocalConnectorWorkspaceId) || null;
    if (!workspace) {
      setLocalConnectorDirectoryError(t('sessionList.resource.error.selectLocalConnectorWorkspace'));
      return;
    }
    const targetPath = joinLocalConnectorDirectoryPath(localConnectorDirectoryPath, name);
    setLocalConnectorDirectoryLoading(true);
    setLocalConnectorDirectoryError(null);
    try {
      const result = await apiClient.createLocalConnectorDirectory({
        device_id: workspace.deviceId,
        workspace_id: workspace.id,
        path: targetPath,
      });
      const nextPath = normalizeLocalConnectorDirectoryPath(result.path || targetPath);
      await loadLocalConnectorDirectoryForWorkspace(workspace, nextPath);
    } catch (error) {
      setLocalConnectorDirectoryError(error instanceof Error ? error.message : t('sessionList.resource.localConnectorDirectoryCreateFailed'));
    } finally {
      setLocalConnectorDirectoryLoading(false);
    }
  }, [
    apiClient,
    loadLocalConnectorDirectoryForWorkspace,
    localConnectorDirectoryPath,
    localConnectorWorkspaces,
    selectedLocalConnectorWorkspaceId,
    t,
  ]);

  return {
    localConnectorWorkspaces,
    localConnectorLoading,
    localConnectorError,
    localConnectorDirectoryPath,
    localConnectorDirectoryParent,
    localConnectorDirectoryEntries,
    localConnectorDirectoryLoading,
    localConnectorDirectoryError,
    selectedLocalConnectorDirectoryPath,
    selectedLocalConnectorWorkspaceId,
    setSelectedLocalConnectorDirectoryPath,
    handleSelectedLocalConnectorWorkspaceChange,
    refreshLocalConnectorWorkspaces,
    browseLocalConnectorDirectory,
    createLocalConnectorDirectory,
  };
};
