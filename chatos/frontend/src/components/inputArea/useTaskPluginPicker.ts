// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';
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
import { taskPluginPickerEnabled } from './pluginRuntimeScope';
import { useDismissiblePopover } from './useDismissiblePopover';

const normalizeError = (error: unknown): string => (
  error instanceof Error ? error.message : String(error || 'Unknown error')
);

const PLUGIN_SELECTION_STORAGE_PREFIX = 'chatos.task-plugin-selection.v4';

interface PersistedTaskPluginSelection {
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
  projectId,
  disabled,
  planMode,
}: {
  client: ApiClient;
  conversationId?: string | null;
  projectId?: string | null;
  disabled: boolean;
  planMode: boolean;
}) => {
  const normalizedProjectId = String(projectId || '').trim();
  const enabled = taskPluginPickerEnabled(conversationId, normalizedProjectId);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [availablePlugins, setAvailablePlugins] = useState<TaskRunnerSelectablePluginResponse[]>([]);
  const [selectedPluginIds, setSelectedPluginIds] = useState<string[]>([]);
  const [selectedCommandInvocations, setSelectedCommandInvocations] = useState<
    PluginCommandInvocationPayload[]
  >([]);
  const [search, setSearch] = useState('');
  const hydratedSelectionScopeRef = useRef<string | null>(null);
  const selectionStorageKey = useMemo(() => (
    conversationId && normalizedProjectId
      ? `${PLUGIN_SELECTION_STORAGE_PREFIX}:${conversationId}:${normalizedProjectId}:${planMode ? 'plan' : 'run'}`
      : null
  ), [conversationId, normalizedProjectId, planMode]);

  const pickerRef = useDismissiblePopover<HTMLDivElement>(open, () => setOpen(false));

  const loadPlugins = useCallback(async () => {
    if (!enabled) {
      setAvailablePlugins([]);
      return;
    }
    const response = await client.listTaskRunnerAvailablePlugins(normalizedProjectId, planMode);
    const plugins = Array.isArray(response?.selectable_plugins)
      ? response.selectable_plugins
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
  }, [client, enabled, normalizedProjectId, planMode]);

  const loadPicker = useCallback(async () => {
    if (!enabled || disabled) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      await loadPlugins();
    } catch (loadError) {
      setError(normalizeError(loadError));
    } finally {
      setLoading(false);
    }
  }, [disabled, enabled, loadPlugins]);

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
    if (!availablePlugins.some((plugin) => plugin.id === pluginId)) {
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
  }, [availablePlugins]);

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
    hydratedSelectionScopeRef.current = null;
    setOpen(false);
    setAvailablePlugins([]);
    setSelectedPluginIds([]);
    setSelectedCommandInvocations([]);
    if (!enabled || !selectionStorageKey) {
      return;
    }
    const persisted = readPersistedSelection(selectionStorageKey);
    setSelectedPluginIds(persisted?.pluginIds || []);
    setSelectedCommandInvocations(persisted?.commandInvocations || []);
    hydratedSelectionScopeRef.current = selectionStorageKey;
    setLoading(true);
    setError(null);
    void loadPlugins()
      .catch((loadError) => setError(normalizeError(loadError)))
      .finally(() => setLoading(false));
  }, [enabled, loadPlugins, selectionStorageKey]);

  useEffect(() => {
    if (!selectionStorageKey
      || hydratedSelectionScopeRef.current !== selectionStorageKey
      || !enabled) {
      return;
    }
    const value: PersistedTaskPluginSelection = {
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
    selectedPluginIds,
    selectionStorageKey,
  ]);

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

  return {
    enabled,
    open,
    pickerRef,
    loading,
    error,
    filteredPlugins,
    selectedPluginIds,
    selectedPlugins,
    availableCommands,
    selectedCommands,
    selectedCommandInvocations,
    pluginCommandInvocations,
    commandArgumentIssue,
    commandMessageFallback,
    search,
    setSearch,
    toggleOpen,
    loadPicker,
    close: () => setOpen(false),
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
