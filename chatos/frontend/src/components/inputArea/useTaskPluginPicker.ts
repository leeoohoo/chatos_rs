// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import { localRuntimeBridgeAvailable } from '../../lib/api/localRuntime';
import type {
  LocalConnectorDeviceResponse,
  LocalConnectorWorkspaceResponse,
  TaskRunnerSelectablePluginAgentResponse,
  TaskRunnerSelectablePluginResponse,
} from '../../lib/api/client/types';
import type {
  PluginAgentSelectionPayload,
  PluginCommandInvocationPayload,
} from '../../types';
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

export interface SelectedTaskPluginCommand extends TaskPluginCommandOption {
  arguments: string;
}

export interface SelectedTaskPluginAgent {
  key: string;
  plugin: TaskRunnerSelectablePluginResponse;
  agent: TaskRunnerSelectablePluginAgentResponse;
}

export const useTaskPluginPicker = ({
  client,
  conversationId,
  disabled,
  planMode,
}: {
  client: ApiClient;
  conversationId?: string | null;
  disabled: boolean;
  planMode: boolean;
}) => {
  const enabled = localRuntimeBridgeAvailable()
    && Boolean(conversationId)
    && !client.sessionUsesLocalRuntime(conversationId);
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
  const [selectedAgentSelection, setSelectedAgentSelection] = useState<
    PluginAgentSelectionPayload | null
  >(null);
  const [search, setSearch] = useState('');

  const pickerRef = useDismissiblePopover<HTMLDivElement>(open, () => setOpen(false));

  const loadPluginsForDevice = useCallback(async (deviceId: string) => {
    const response = await client.listTaskRunnerAvailablePlugins(deviceId, planMode);
    const plugins = Array.isArray(response?.selectable_plugins)
      ? response.selectable_plugins.filter((plugin) => plugin.device_id === deviceId)
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
    setSelectedAgentSelection((current) => {
      if (!current) {
        return null;
      }
      const plugin = plugins.find((item) => item.id === current.plugin_id);
      return plugin && (Array.isArray(plugin.agents) ? plugin.agents : []).some((agent) => (
        agent.agent_id === current.agent_id
      ))
        ? current
        : null;
    });
  }, [client, planMode]);

  const selectDevice = useCallback(async (deviceId: string) => {
    const normalized = deviceId.trim();
    setSelectedDeviceId(normalized || null);
    setSelectedPluginIds([]);
    setSelectedCommandInvocations([]);
    setSelectedAgentSelection(null);
    setAvailablePlugins([]);
    setError(null);
    const matchingWorkspaces = workspaces.filter((workspace) => (
      workspaceDeviceId(workspace) === normalized
    ));
    setSelectedWorkspaceId(matchingWorkspaces[0]?.id || null);
    if (!normalized) {
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
  }, [loadPluginsForDevice, workspaces]);

  const loadPicker = useCallback(async () => {
    if (!enabled || disabled) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const [deviceItems, workspaceItems] = await Promise.all([
        client.listLocalConnectorDevices(),
        client.listLocalConnectorWorkspaces(),
      ]);
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
        : (onlineDevices[0]?.id || null);
      setSelectedDeviceId(nextDeviceId);
      const matchingWorkspaces = activeWorkspaces.filter((workspace) => (
        workspaceDeviceId(workspace) === nextDeviceId
      ));
      const currentWorkspaceAvailable = selectedWorkspaceId
        && matchingWorkspaces.some((workspace) => workspace.id === selectedWorkspaceId);
      setSelectedWorkspaceId(currentWorkspaceAvailable
        ? selectedWorkspaceId
        : (matchingWorkspaces[0]?.id || null));
      if (nextDeviceId) {
        await loadPluginsForDevice(nextDeviceId);
      } else {
        setAvailablePlugins([]);
        setSelectedPluginIds([]);
        setSelectedCommandInvocations([]);
        setSelectedAgentSelection(null);
      }
    } catch (loadError) {
      setError(normalizeError(loadError));
    } finally {
      setLoading(false);
    }
  }, [client, disabled, enabled, loadPluginsForDevice, selectedDeviceId, selectedWorkspaceId]);

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
    setSelectedCommandInvocations((commands) => commands.filter((command) => (
      command.plugin_id !== pluginId
    )));
    setSelectedPluginIds((current) => (
      current.includes(pluginId)
        ? current.filter((value) => value !== pluginId)
        : [...current, pluginId]
    ));
    setSelectedAgentSelection((current) => (
      current?.plugin_id === pluginId ? null : current
    ));
  }, []);

  const selectPlugin = useCallback((pluginId: string) => {
    if (!availablePlugins.some((plugin) => plugin.id === pluginId)) {
      return;
    }
    setSelectedPluginIds((current) => (
      current.includes(pluginId) ? current : [...current, pluginId]
    ));
  }, [availablePlugins]);

  const clearSelectedPlugins = useCallback(() => {
    setSelectedPluginIds([]);
    setSelectedCommandInvocations([]);
    setSelectedAgentSelection(null);
  }, []);

  const selectAgent = useCallback((pluginId: string, agentId: string) => {
    const plugin = availablePlugins.find((item) => item.id === pluginId);
    const agent = (Array.isArray(plugin?.agents) ? plugin.agents : [])
      .find((item) => item.agent_id === agentId);
    if (!plugin || !agent) {
      return;
    }
    setSelectedPluginIds((current) => (
      current.includes(pluginId) ? current : [...current, pluginId]
    ));
    setSelectedAgentSelection({
      plugin_id: pluginId,
      agent_id: agentId,
    });
  }, [availablePlugins]);

  const clearSelectedAgent = useCallback(() => {
    setSelectedAgentSelection(null);
  }, []);

  const selectCommand = useCallback((
    pluginId: string,
    commandId: string,
    argumentsText = '',
  ) => {
    const plugin = availablePlugins.find((item) => item.id === pluginId);
    const command = (Array.isArray(plugin?.commands) ? plugin.commands : [])
      .find((item) => item.command_id === commandId);
    if (!plugin || !command) {
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
  }, [availablePlugins]);

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
    if (enabled) {
      return;
    }
    setOpen(false);
    setDevices([]);
    setWorkspaces([]);
    setAvailablePlugins([]);
    setSelectedDeviceId(null);
    setSelectedWorkspaceId(null);
    setSelectedPluginIds([]);
    setSelectedCommandInvocations([]);
    setSelectedAgentSelection(null);
  }, [enabled]);

  useEffect(() => {
    setOpen(false);
    setAvailablePlugins([]);
    setSelectedPluginIds([]);
    setSelectedCommandInvocations([]);
    setSelectedAgentSelection(null);
  }, [planMode]);

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
      )).join(' ')} ${(
        Array.isArray(plugin.agents) ? plugin.agents : []
      ).map((agent) => (
        `${agent.agent_id} ${agent.display_name} ${agent.description || ''} ${agent.base_agent}`
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
  const selectedAgent = useMemo<SelectedTaskPluginAgent | null>(() => {
    if (!selectedAgentSelection) {
      return null;
    }
    const plugin = availablePlugins.find((item) => (
      item.id === selectedAgentSelection.plugin_id
    ));
    const agent = (Array.isArray(plugin?.agents) ? plugin.agents : []).find((item) => (
      item.agent_id === selectedAgentSelection.agent_id
    ));
    return plugin && agent ? {
      key: `${plugin.id}\u0000${agent.agent_id}`,
      plugin,
      agent,
    } : null;
  }, [availablePlugins, selectedAgentSelection]);
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
  const browserWorkspaceRequired = selectedPlugins.some((plugin) => plugin.plugin_key === 'browser')
    && !selectedWorkspaceId;

  return {
    enabled,
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
    selectedAgentSelection,
    selectedAgent,
    commandArgumentIssue,
    commandMessageFallback,
    search,
    browserWorkspaceRequired,
    setSearch,
    setSelectedWorkspaceId,
    toggleOpen,
    loadPicker,
    close: () => setOpen(false),
    selectDevice,
    selectPlugin,
    togglePlugin,
    removePlugin: togglePlugin,
    selectAgent,
    clearSelectedAgent,
    selectCommand,
    toggleCommand,
    removeCommand,
    setCommandArguments,
    commandSuggestions,
    pluginSuggestions,
    clearSelectedPlugins,
  };
};
