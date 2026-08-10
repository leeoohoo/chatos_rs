// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import { localRuntimeBridgeAvailable } from '../../lib/api/localRuntime';
import type {
  LocalConnectorDeviceResponse,
  LocalConnectorWorkspaceResponse,
  TaskRunnerSelectablePluginResponse,
} from '../../lib/api/client/types';
import type { PluginCommandInvocationPayload } from '../../types';
import {
  filterPluginCommandOptions,
  MAX_PLUGIN_COMMAND_ARGUMENT_BYTES,
  MAX_PLUGIN_COMMAND_INVOCATIONS,
  pluginCommandKey,
  pluginCommandOptions,
  type TaskPluginCommandOption,
  utf8ByteLength,
} from './pluginCommands';
import { filterPluginMentionOptions } from './pluginMentions';
import {
  filterPluginsForRuntime,
  pluginRequiresLocalWorkspace,
  pluginUsesLocalConnector,
  taskPluginPickerEnabled,
  type TaskPluginRuntimeProvider,
} from './pluginRuntimeScope';
import { useDismissiblePopover } from './useDismissiblePopover';

const deviceStatus = (device: LocalConnectorDeviceResponse): string => (
  String(device.status || '').trim().toLowerCase()
);

const workspaceStatus = (workspace: LocalConnectorWorkspaceResponse): string => (
  String(workspace.status || '').trim().toLowerCase()
);

const workspaceDeviceId = (workspace: LocalConnectorWorkspaceResponse): string => (
  String(workspace.device_id || workspace.deviceId || '').trim()
);

const normalizeError = (error: unknown): string => (
  error instanceof Error ? error.message : String(error || 'Unknown error')
);

const PLUGIN_SELECTION_STORAGE_PREFIX = 'chatos.task-plugin-selection.v3';

interface PersistedTaskPluginSelection {
  deviceId: string | null;
  workspaceId: string | null;
  pluginIds: string[];
  commandInvocations: PluginCommandInvocationPayload[];
}

const readPersistedSelection = (key: string): PersistedTaskPluginSelection | null => {
  try {
    const value = JSON.parse(window.localStorage.getItem(key) || 'null') as Partial<
      PersistedTaskPluginSelection
    > | null;
    if (!value || !Array.isArray(value.pluginIds) || !Array.isArray(value.commandInvocations)) {
      return null;
    }
    return {
      deviceId: typeof value.deviceId === 'string' ? value.deviceId : null,
      workspaceId: typeof value.workspaceId === 'string' ? value.workspaceId : null,
      pluginIds: value.pluginIds.filter((item): item is string => typeof item === 'string'),
      commandInvocations: value.commandInvocations.filter((item) => (
        item
        && typeof item.plugin_id === 'string'
        && typeof item.command_id === 'string'
      )),
    };
  } catch {
    return null;
  }
};

export interface SelectedTaskPluginCommand extends TaskPluginCommandOption {
  arguments: string;
}

export const useTaskPluginPicker = ({
  client,
  conversationId,
  disabled,
  planMode,
  localConnectorEnabled,
}: {
  client: ApiClient;
  conversationId?: string | null;
  disabled: boolean;
  planMode: boolean;
  localConnectorEnabled: boolean;
}) => {
  const enabled = taskPluginPickerEnabled(conversationId, localConnectorEnabled);
  const runtimeProvider: TaskPluginRuntimeProvider = localConnectorEnabled
    ? 'local_connector'
    : 'cloud';
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [devices, setDevices] = useState<LocalConnectorDeviceResponse[]>([]);
  const [workspaces, setWorkspaces] = useState<LocalConnectorWorkspaceResponse[]>([]);
  const [availablePlugins, setAvailablePlugins] = useState<TaskRunnerSelectablePluginResponse[]>([]);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string | null>(null);
  const [selectedPluginIds, setSelectedPluginIds] = useState<string[]>([]);
  const [selectedCommandInvocations, setSelectedCommandInvocations] = useState<
    PluginCommandInvocationPayload[]
  >([]);
  const [search, setSearch] = useState('');
  const hydratedSelectionScopeRef = useRef<string | null>(null);
  const selectionStorageKey = useMemo(() => (
    conversationId
      ? `${PLUGIN_SELECTION_STORAGE_PREFIX}:${conversationId}:${planMode ? 'plan' : 'run'}:${runtimeProvider}`
      : null
  ), [conversationId, planMode, runtimeProvider]);

  const pickerRef = useDismissiblePopover<HTMLDivElement>(open, () => setOpen(false));

  const loadPluginsForDevice = useCallback(async (deviceId: string | null) => {
    const effectiveDeviceId = localConnectorEnabled ? deviceId : null;
    const response = await client.listTaskRunnerAvailablePlugins(
      runtimeProvider,
      effectiveDeviceId,
      planMode,
    );
    const plugins = Array.isArray(response?.selectable_plugins)
      ? filterPluginsForRuntime(
        response.selectable_plugins,
        runtimeProvider,
        effectiveDeviceId,
      )
      : [];
    setAvailablePlugins(plugins);
    setSelectedPluginIds((current) => current.filter((pluginId) => (
      plugins.some((plugin) => plugin.id === pluginId)
    )));
    setSelectedCommandInvocations((current) => current.filter((invocation) => (
      plugins.some((plugin) => (
        plugin.id === invocation.plugin_id
        && (Array.isArray(plugin.commands) ? plugin.commands : []).some((command) => (
          command.command_id === invocation.command_id
        ))
      ))
    )));
  }, [client, localConnectorEnabled, planMode, runtimeProvider]);

  const selectDevice = useCallback(async (deviceId: string) => {
    if (!localConnectorEnabled) {
      return;
    }
    const normalized = deviceId.trim();
    setSelectedDeviceId(normalized || null);
    const deviceDependentPluginIds = new Set(availablePlugins
      .filter((plugin) => plugin.requires_device)
      .map((plugin) => plugin.id));
    setSelectedPluginIds((current) => current.filter((pluginId) => (
      !deviceDependentPluginIds.has(pluginId)
    )));
    setSelectedCommandInvocations((current) => current.filter((invocation) => (
      !deviceDependentPluginIds.has(invocation.plugin_id)
    )));
    setError(null);
    const matchingWorkspaces = workspaces.filter((workspace) => (
      workspaceDeviceId(workspace) === normalized
    ));
    setSelectedWorkspaceId(matchingWorkspaces[0]?.id || null);
    if (!normalized) {
      setSelectedWorkspaceId(null);
      setLoading(true);
      try {
        await loadPluginsForDevice(null);
      } catch (loadError) {
        setError(normalizeError(loadError));
      } finally {
        setLoading(false);
      }
      return;
    }
    setLoading(true);
    try {
      await loadPluginsForDevice(normalized);
    } catch (loadError) {
      setError(normalizeError(loadError));
    } finally {
      setLoading(false);
    }
  }, [availablePlugins, loadPluginsForDevice, localConnectorEnabled, workspaces]);

  const loadPicker = useCallback(async () => {
    if (!enabled || disabled) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const [deviceItems, workspaceItems] = localConnectorEnabled && localRuntimeBridgeAvailable()
        ? await Promise.all([
          client.listLocalConnectorDevices(),
          client.listLocalConnectorWorkspaces(),
        ])
        : [[], []];
      const onlineDevices = (Array.isArray(deviceItems) ? deviceItems : [])
        .filter((device) => deviceStatus(device) === 'online');
      const activeWorkspaces = (Array.isArray(workspaceItems) ? workspaceItems : [])
        .filter((workspace) => workspaceStatus(workspace) === 'active');
      setDevices(onlineDevices);
      setWorkspaces(activeWorkspaces);

      const currentDeviceAvailable = selectedDeviceId
        && onlineDevices.some((device) => device.id === selectedDeviceId);
      const nextDeviceId = currentDeviceAvailable
        ? selectedDeviceId
        : null;
      setSelectedDeviceId(nextDeviceId);
      const matchingWorkspaces = activeWorkspaces.filter((workspace) => (
        workspaceDeviceId(workspace) === nextDeviceId
      ));
      const currentWorkspaceAvailable = selectedWorkspaceId
        && matchingWorkspaces.some((workspace) => workspace.id === selectedWorkspaceId);
      setSelectedWorkspaceId(currentWorkspaceAvailable
        ? selectedWorkspaceId
        : (matchingWorkspaces[0]?.id || null));
      await loadPluginsForDevice(nextDeviceId);
    } catch (loadError) {
      setError(normalizeError(loadError));
    } finally {
      setLoading(false);
    }
  }, [
    client,
    disabled,
    enabled,
    loadPluginsForDevice,
    localConnectorEnabled,
    selectedDeviceId,
    selectedWorkspaceId,
  ]);

  const toggleOpen = useCallback(() => {
    if (!enabled || disabled) {
      return;
    }
    if (open) {
      setOpen(false);
      return;
    }
    setOpen(true);
    void loadPicker();
  }, [disabled, enabled, loadPicker, open]);

  const togglePlugin = useCallback((pluginId: string) => {
    const plugin = availablePlugins.find((item) => item.id === pluginId);
    if (!plugin) {
      return;
    }
    if (!selectedPluginIds.includes(pluginId) && plugin.requires_device && !selectedDeviceId) {
      setError('该插件包含本地执行组件，请先选择 Local Connector 设备。');
      return;
    }
    setError(null);
    setSelectedCommandInvocations((commands) => commands.filter((command) => (
      command.plugin_id !== pluginId
    )));
    setSelectedPluginIds((current) => (
      current.includes(pluginId)
        ? current.filter((value) => value !== pluginId)
        : [...current, pluginId]
    ));
  }, [availablePlugins, selectedDeviceId, selectedPluginIds]);

  const selectPlugin = useCallback((pluginId: string) => {
    if (!availablePlugins.some((plugin) => plugin.id === pluginId)) {
      return;
    }
    const plugin = availablePlugins.find((item) => item.id === pluginId);
    if (plugin?.requires_device && !selectedDeviceId) {
      setError('该插件包含本地执行组件，请先选择 Local Connector 设备。');
      return;
    }
    setSelectedPluginIds((current) => (
      current.includes(pluginId) ? current : [...current, pluginId]
    ));
  }, [availablePlugins, selectedDeviceId]);

  const clearSelectedPlugins = useCallback(() => {
    setSelectedPluginIds([]);
    setSelectedCommandInvocations([]);
  }, []);

  const selectCommand = useCallback((
    pluginId: string,
    commandId: string,
    argumentsText = '',
  ) => {
    const plugin = availablePlugins.find((item) => item.id === pluginId);
    const command = (Array.isArray(plugin?.commands) ? plugin.commands : [])
      .find((item) => item.command_id === commandId);
    if (!plugin || !command || (plugin.requires_device && !selectedDeviceId)) {
      if (plugin?.requires_device && !selectedDeviceId) {
        setError('该插件包含本地执行组件，请先选择 Local Connector 设备。');
      }
      return;
    }
    setSelectedPluginIds((current) => (
      current.includes(pluginId) ? current : [...current, pluginId]
    ));
    setSelectedCommandInvocations((current) => {
      const key = pluginCommandKey(pluginId, commandId);
      const next = current.filter((item) => (
        pluginCommandKey(item.plugin_id, item.command_id) !== key
      ));
      if (next.length >= MAX_PLUGIN_COMMAND_INVOCATIONS) {
        return current;
      }
      return [...next, {
        plugin_id: pluginId,
        command_id: commandId,
        arguments: argumentsText,
      }];
    });
  }, [availablePlugins, selectedDeviceId]);

  const removeCommand = useCallback((pluginId: string, commandId: string) => {
    const key = pluginCommandKey(pluginId, commandId);
    setSelectedCommandInvocations((current) => current.filter((item) => (
      pluginCommandKey(item.plugin_id, item.command_id) !== key
    )));
  }, []);

  const toggleCommand = useCallback((pluginId: string, commandId: string) => {
    const key = pluginCommandKey(pluginId, commandId);
    const selected = selectedCommandInvocations.some((item) => (
      pluginCommandKey(item.plugin_id, item.command_id) === key
    ));
    if (selected) {
      removeCommand(pluginId, commandId);
      return;
    }
    selectCommand(pluginId, commandId);
  }, [removeCommand, selectCommand, selectedCommandInvocations]);

  const setCommandArguments = useCallback((
    pluginId: string,
    commandId: string,
    argumentsText: string,
  ) => {
    const key = pluginCommandKey(pluginId, commandId);
    setSelectedCommandInvocations((current) => current.map((item) => (
      pluginCommandKey(item.plugin_id, item.command_id) === key
        ? { ...item, arguments: argumentsText }
        : item
    )));
  }, []);

  useEffect(() => {
    hydratedSelectionScopeRef.current = null;
    setOpen(false);
    setDevices([]);
    setWorkspaces([]);
    setAvailablePlugins([]);
    setSelectedDeviceId(null);
    setSelectedWorkspaceId(null);
    setSelectedPluginIds([]);
    setSelectedCommandInvocations([]);
    if (!enabled || !selectionStorageKey) {
      return;
    }
    const persisted = readPersistedSelection(selectionStorageKey);
    const deviceId = persisted?.deviceId || null;
    setSelectedDeviceId(deviceId);
    setSelectedWorkspaceId(persisted?.workspaceId || null);
    setSelectedPluginIds(persisted?.pluginIds || []);
    setSelectedCommandInvocations(persisted?.commandInvocations || []);
    hydratedSelectionScopeRef.current = selectionStorageKey;
    setLoading(true);
    setError(null);
    void loadPluginsForDevice(deviceId)
      .catch((loadError) => setError(normalizeError(loadError)))
      .finally(() => setLoading(false));
  }, [enabled, loadPluginsForDevice, selectionStorageKey]);

  useEffect(() => {
    if (!selectionStorageKey
      || hydratedSelectionScopeRef.current !== selectionStorageKey
      || !enabled) {
      return;
    }
    const value: PersistedTaskPluginSelection = {
      deviceId: selectedDeviceId,
      workspaceId: selectedWorkspaceId,
      pluginIds: selectedPluginIds,
      commandInvocations: selectedCommandInvocations,
    };
    try {
      window.localStorage.setItem(selectionStorageKey, JSON.stringify(value));
    } catch {
      // Selection remains valid for the current conversation even if persistence is unavailable.
    }
  }, [
    enabled,
    selectedCommandInvocations,
    selectedDeviceId,
    selectedPluginIds,
    selectedWorkspaceId,
    selectionStorageKey,
  ]);

  const deviceWorkspaces = useMemo(() => workspaces.filter((workspace) => (
    workspaceDeviceId(workspace) === selectedDeviceId
  )), [selectedDeviceId, workspaces]);
  const filteredPlugins = useMemo(() => {
    const keyword = search.trim().toLowerCase();
    if (!keyword) {
      return availablePlugins;
    }
    return availablePlugins.filter((plugin) => (
      `${plugin.display_name} ${plugin.plugin_key} ${plugin.description} ${(
        Array.isArray(plugin.commands) ? plugin.commands : []
      ).map((command) => (
        `${command.command_id} ${command.display_name} ${command.description || ''}`
      )).join(' ')}`
        .toLowerCase()
        .includes(keyword)
    ));
  }, [availablePlugins, search]);
  const selectedPlugins = useMemo(() => selectedPluginIds.flatMap((pluginId) => {
    const plugin = availablePlugins.find((item) => item.id === pluginId);
    return plugin ? [plugin] : [];
  }), [availablePlugins, selectedPluginIds]);
  const availableCommands = useMemo(
    () => pluginCommandOptions(availablePlugins),
    [availablePlugins],
  );
  const selectedCommands = useMemo<SelectedTaskPluginCommand[]>(() => (
    selectedCommandInvocations.flatMap((invocation) => {
      const key = pluginCommandKey(invocation.plugin_id, invocation.command_id);
      const option = availableCommands.find((item) => item.key === key);
      return option ? [{
        ...option,
        arguments: typeof invocation.arguments === 'string' ? invocation.arguments : '',
      }] : [];
    })
  ), [availableCommands, selectedCommandInvocations]);
  const commandArgumentIssue = useMemo(() => selectedCommands.find(({ arguments: value }) => (
    value.includes('\0') || utf8ByteLength(value.trim()) > MAX_PLUGIN_COMMAND_ARGUMENT_BYTES
  )) || null, [selectedCommands]);
  const pluginCommandInvocations = useMemo<PluginCommandInvocationPayload[]>(() => (
    selectedCommands.map(({ plugin, command, arguments: value }) => ({
      plugin_id: plugin.id,
      command_id: command.command_id,
      arguments: value.trim() || null,
    }))
  ), [selectedCommands]);
  const commandMessageFallback = useMemo(() => selectedCommands
    .map(({ command }) => `/${command.command_id}`)
    .join(' '), [selectedCommands]);
  const commandSuggestions = useCallback((query: string) => (
    filterPluginCommandOptions(availableCommands, query)
  ), [availableCommands]);
  const pluginSuggestions = useCallback((query: string) => (
    filterPluginMentionOptions(availablePlugins, query)
  ), [availablePlugins]);
  const requiresDevice = selectedPlugins.some(pluginUsesLocalConnector);
  const workspaceRequired = selectedPlugins.some(pluginRequiresLocalWorkspace)
    && !selectedWorkspaceId;

  return {
    enabled,
    localConnectorEnabled,
    open,
    pickerRef,
    loading,
    error,
    devices,
    deviceWorkspaces,
    filteredPlugins,
    selectedDeviceId,
    selectedWorkspaceId,
    selectedPluginIds,
    selectedPlugins,
    availableCommands,
    selectedCommands,
    selectedCommandInvocations,
    pluginCommandInvocations,
    commandArgumentIssue,
    commandMessageFallback,
    search,
    requiresDevice,
    workspaceRequired,
    setSearch,
    setSelectedWorkspaceId,
    toggleOpen,
    loadPicker,
    close: () => setOpen(false),
    selectDevice,
    selectPlugin,
    togglePlugin,
    removePlugin: togglePlugin,
    selectCommand,
    toggleCommand,
    removeCommand,
    setCommandArguments,
    commandSuggestions,
    pluginSuggestions,
    clearSelectedPlugins,
  };
};
