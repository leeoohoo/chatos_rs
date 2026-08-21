// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import type {
  LocalConnectorDeviceResponse,
  TaskRunnerSelectablePluginResponse,
} from '../../lib/api/client/types';
import { PUBLIC_PROJECT_ID } from '../../lib/domain/contactSessions';
import { normalizeProject } from '../../lib/domain/projects';
import type { Project } from '../../types';
import { filterPluginMentionOptions } from './pluginMentions';
import {
  filterPluginsForProjectRuntime,
  resolveTaskPluginRuntimeScope,
} from './pluginRuntimeScope';
import { useDismissiblePopover } from './useDismissiblePopover';

const normalizeError = (error: unknown): string => (
  error instanceof Error ? error.message : String(error || 'Unknown error')
);

const PLUGIN_SELECTION_STORAGE_PREFIX = 'chatos.task-plugin-preferences.v5';

interface PersistedTaskPluginSelection {
  pluginIds: string[];
}

const readPersistedSelection = (key: string): PersistedTaskPluginSelection | null => {
  try {
    const value = JSON.parse(window.localStorage.getItem(key) || 'null') as Partial<
      PersistedTaskPluginSelection
    > | null;
    if (!value || !Array.isArray(value.pluginIds)) {
      return null;
    }
    return {
      pluginIds: value.pluginIds.filter((item): item is string => typeof item === 'string'),
    };
  } catch {
    return null;
  }
};

export const useTaskPluginPicker = ({
  client,
  conversationId,
  project,
  projectId,
  disabled,
  planMode,
}: {
  client: ApiClient;
  conversationId?: string | null;
  project?: Project | null;
  projectId?: string | null;
  disabled: boolean;
  planMode: boolean;
}) => {
  const normalizedProjectId = String(projectId || project?.id || '').trim();
  const suppliedProject = project?.id === normalizedProjectId ? project : null;
  const [loadedProject, setLoadedProject] = useState<Project | null>(null);
  const resolvedProject = suppliedProject
    || (loadedProject?.id === normalizedProjectId ? loadedProject : null);
  const runtimeScope = useMemo(
    () => resolveTaskPluginRuntimeScope(conversationId, resolvedProject),
    [conversationId, resolvedProject],
  );
  const enabled = Boolean(runtimeScope);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [catalogResolved, setCatalogResolved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [availablePlugins, setAvailablePlugins] = useState<TaskRunnerSelectablePluginResponse[]>([]);
  const [selectedPluginIds, setSelectedPluginIds] = useState<string[]>([]);
  const [search, setSearch] = useState('');
  const hydratedSelectionScopeRef = useRef<string | null>(null);
  const activeRuntimeScopeRef = useRef<string | null>(null);
  const runtimeScopeKey = runtimeScope
    ? `${runtimeScope.projectId}:${runtimeScope.sourceKind}:${runtimeScope.localDeviceId || ''}:${planMode ? 'plan' : 'run'}`
    : null;
  activeRuntimeScopeRef.current = runtimeScopeKey;
  const selectionStorageKey = useMemo(() => (
    conversationId && runtimeScope?.projectId
      ? `${PLUGIN_SELECTION_STORAGE_PREFIX}:${conversationId}:${runtimeScope.projectId}:${planMode ? 'plan' : 'run'}`
      : null
  ), [conversationId, planMode, runtimeScope?.projectId]);

  const pickerRef = useDismissiblePopover<HTMLDivElement>(open, () => setOpen(false));

  useEffect(() => {
    if (
      suppliedProject
      || !conversationId
      || !normalizedProjectId
      || normalizedProjectId === '0'
      || normalizedProjectId === PUBLIC_PROJECT_ID
    ) {
      setLoadedProject(null);
      return undefined;
    }

    let active = true;
    setLoadedProject(null);
    void client.getProject(normalizedProjectId)
      .then((response) => {
        if (!active) {
          return;
        }
        const nextProject = normalizeProject(response);
        setLoadedProject(nextProject.id === normalizedProjectId ? nextProject : null);
      })
      .catch(() => {
        if (active) {
          setLoadedProject(null);
        }
      });
    return () => {
      active = false;
    };
  }, [client, conversationId, normalizedProjectId, suppliedProject]);

  const loadPlugins = useCallback(async (): Promise<boolean> => {
    if (!runtimeScope) {
      setAvailablePlugins([]);
      return false;
    }
    const requestedScopeKey = runtimeScopeKey;
    if (runtimeScope.sourceKind === 'local_connector') {
      const devices = await client.listLocalConnectorDevices();
      if (activeRuntimeScopeRef.current !== requestedScopeKey) {
        return false;
      }
      const localDeviceOnline = (Array.isArray(devices) ? devices : []).some((device) => (
        String(device.id || '').trim() === runtimeScope.localDeviceId
        && String((device as LocalConnectorDeviceResponse).status || '').trim().toLowerCase() === 'online'
      ));
      if (!localDeviceOnline) {
        setAvailablePlugins([]);
        setSelectedPluginIds([]);
        return true;
      }
    }
    const response = await client.listTaskRunnerAvailablePlugins(runtimeScope.projectId, planMode);
    if (activeRuntimeScopeRef.current !== requestedScopeKey) {
      return false;
    }
    const plugins = Array.isArray(response?.selectable_plugins)
      ? filterPluginsForProjectRuntime(response.selectable_plugins, runtimeScope.sourceKind)
      : [];
    setAvailablePlugins(plugins);
    setSelectedPluginIds((current) => current.filter((pluginId) => (
      plugins.some((plugin) => plugin.id === pluginId)
    )));
    return true;
  }, [client, planMode, runtimeScope, runtimeScopeKey]);

  const loadPicker = useCallback(async () => {
    if (!enabled || disabled) {
      return;
    }
    setLoading(true);
    setError(null);
    const requestedScopeKey = runtimeScopeKey;
    try {
      const current = await loadPlugins();
      if (!current) {
        return;
      }
    } catch (loadError) {
      if (activeRuntimeScopeRef.current !== requestedScopeKey) {
        return;
      }
      setError(normalizeError(loadError));
    } finally {
      if (activeRuntimeScopeRef.current === requestedScopeKey) {
        setLoading(false);
        setCatalogResolved(true);
      }
    }
  }, [disabled, enabled, loadPlugins, runtimeScopeKey]);

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
  }, []);

  useEffect(() => {
    hydratedSelectionScopeRef.current = null;
    setOpen(false);
    setCatalogResolved(false);
    setAvailablePlugins([]);
    setSelectedPluginIds([]);
    if (!enabled || !selectionStorageKey) {
      return;
    }
    const persisted = readPersistedSelection(selectionStorageKey);
    setSelectedPluginIds(persisted?.pluginIds || []);
    hydratedSelectionScopeRef.current = selectionStorageKey;
    setLoading(true);
    setError(null);
    const requestedScopeKey = runtimeScopeKey;
    void loadPlugins()
      .catch((loadError) => {
        if (activeRuntimeScopeRef.current === requestedScopeKey) {
          setError(normalizeError(loadError));
        }
      })
      .finally(() => {
        if (activeRuntimeScopeRef.current === requestedScopeKey) {
          setLoading(false);
          setCatalogResolved(true);
        }
      });
  }, [enabled, loadPlugins, runtimeScopeKey, selectionStorageKey]);

  useEffect(() => {
    if (!selectionStorageKey
      || hydratedSelectionScopeRef.current !== selectionStorageKey
      || !enabled) {
      return;
    }
    const value: PersistedTaskPluginSelection = {
      pluginIds: selectedPluginIds,
    };
    try {
      window.localStorage.setItem(selectionStorageKey, JSON.stringify(value));
    } catch {
      // Selection remains valid for the current conversation even if persistence is unavailable.
    }
  }, [
    enabled,
    selectedPluginIds,
    selectionStorageKey,
  ]);

  const filteredPlugins = useMemo(() => {
    const keyword = search.trim().toLowerCase();
    if (!keyword) {
      return availablePlugins;
    }
    return availablePlugins.filter((plugin) => (
      `${plugin.display_name} ${plugin.plugin_key} ${plugin.description}`
        .toLowerCase()
        .includes(keyword)
    ));
  }, [availablePlugins, search]);
  const effectiveSelectedPluginIds = catalogResolved && !error ? selectedPluginIds : [];
  const selectedPlugins = useMemo(() => effectiveSelectedPluginIds.flatMap((pluginId) => {
    const plugin = availablePlugins.find((item) => item.id === pluginId);
    return plugin ? [plugin] : [];
  }), [availablePlugins, effectiveSelectedPluginIds]);
  const pluginSuggestions = useCallback((query: string) => (
    filterPluginMentionOptions(availablePlugins, query)
  ), [availablePlugins]);

  return {
    enabled,
    visible: enabled && catalogResolved && !error && availablePlugins.length > 0,
    open,
    pickerRef,
    loading,
    error,
    filteredPlugins,
    selectedPluginIds: effectiveSelectedPluginIds,
    selectedPlugins,
    search,
    setSearch,
    toggleOpen,
    loadPicker,
    close: () => setOpen(false),
    selectPlugin,
    togglePlugin,
    removePlugin: togglePlugin,
    pluginSuggestions,
    clearSelectedPlugins,
  };
};
