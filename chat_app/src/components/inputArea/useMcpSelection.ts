import { useCallback, useEffect, useMemo, useState, type Dispatch, type SetStateAction } from 'react';

import {
  PROJECT_REQUIRED_MCP_IDS,
  REMOTE_REQUIRED_MCP_IDS,
} from './mcpSelection/mcpSelectionConstants';
import { normalizeSelectableMcpConfigs } from './mcpSelection/mcpConfigNormalization';
import {
  normalizeEnabledMcpIdList,
  normalizeProjectScopeKey,
  readProjectDefaultMap,
  writeProjectDefaultMap,
} from './mcpSelection/mcpProjectDefaults';
import {
  buildSelectableMcpIds,
  sanitizeEnabledMcpIds,
  splitMcpConfigsByBuiltin,
} from './mcpSelection/mcpSelectionState';
import { buildMcpToolsetPresets } from './mcpSelection/mcpToolsetPresets';
import type {
  McpToolsetPreset,
  SelectableMcpConfig,
} from './mcpSelection/mcpSelectionTypes';

export type {
  McpToolsetPreset,
  SelectableMcpConfig,
} from './mcpSelection/mcpSelectionTypes';
export { buildMcpToolsetPresets } from './mcpSelection/mcpToolsetPresets';

interface McpApiClient {
  getMcpConfigs: (userId?: string, options?: { forceRefresh?: boolean }) => Promise<unknown>;
}

interface UseMcpSelectionOptions {
  client: McpApiClient;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  projectScopeKey?: string | null;
  hasDirectoryContext: boolean;
  hasRemoteContext: boolean;
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  onMcpEnabledChange?: (enabled: boolean) => void;
  onEnabledMcpIdsChange?: (ids: string[]) => void;
}

interface UseMcpSelectionResult {
  mcpPickerOpen: boolean;
  setMcpPickerOpen: Dispatch<SetStateAction<boolean>>;
  availableMcpConfigs: SelectableMcpConfig[];
  mcpConfigsLoading: boolean;
  mcpConfigsError: string | null;
  builtinMcpConfigs: SelectableMcpConfig[];
  customMcpConfigs: SelectableMcpConfig[];
  mcpToolsetPresets: McpToolsetPreset[];
  projectScopeKey: string | null;
  hasProjectMcpDefault: boolean;
  selectableMcpIds: string[];
  sanitizedEnabledMcpIds: string[];
  isAllMcpSelected: boolean;
  selectedMcpCount: number;
  isProjectRequiredMcpId: (id: string) => boolean;
  isRemoteRequiredMcpId: (id: string) => boolean;
  loadAvailableMcpConfigs: () => Promise<void>;
  handleToggleMcpPicker: () => void;
  handleSelectAllMcp: () => void;
  handleToggleMcpSelection: (mcpId: string) => void;
  handleApplyMcpToolsetPreset: (presetId: string) => void;
  handleSaveProjectMcpDefault: () => void;
  handleApplyProjectMcpDefault: () => void;
}

export const useMcpSelection = ({
  client,
  mcpEnabled,
  enabledMcpIds,
  projectScopeKey,
  hasDirectoryContext,
  hasRemoteContext,
  disabled,
  isStreaming,
  isStopping,
  onMcpEnabledChange,
  onEnabledMcpIdsChange,
}: UseMcpSelectionOptions): UseMcpSelectionResult => {
  const [mcpPickerOpen, setMcpPickerOpen] = useState(false);
  const [availableMcpConfigs, setAvailableMcpConfigs] = useState<SelectableMcpConfig[]>([]);
  const [mcpConfigsLoading, setMcpConfigsLoading] = useState(false);
  const [mcpConfigsError, setMcpConfigsError] = useState<string | null>(null);
  const [projectDefaultsVersion, setProjectDefaultsVersion] = useState(0);
  const normalizedProjectScopeKey = useMemo(
    () => normalizeProjectScopeKey(projectScopeKey),
    [projectScopeKey],
  );

  const availableMcpIds = useMemo(
    () => availableMcpConfigs.map((item) => item.id),
    [availableMcpConfigs],
  );
  const selectableMcpIds = useMemo(
    () => buildSelectableMcpIds(availableMcpIds, hasDirectoryContext, hasRemoteContext),
    [availableMcpIds, hasDirectoryContext, hasRemoteContext],
  );
  const selectableMcpIdSet = useMemo(
    () => new Set(selectableMcpIds),
    [selectableMcpIds],
  );
  const allowAllShortcut = useMemo(
    () => availableMcpIds.length > 0 && selectableMcpIds.length === availableMcpIds.length,
    [availableMcpIds.length, selectableMcpIds.length],
  );
  const sanitizedEnabledMcpIds = useMemo(() => sanitizeEnabledMcpIds({
    availableMcpIds,
    enabledMcpIds,
    selectableMcpIds,
    selectableMcpIdSet,
    allowAllShortcut,
  }), [
    allowAllShortcut,
    availableMcpIds,
    enabledMcpIds,
    selectableMcpIdSet,
    selectableMcpIds,
  ]);
  const isAllMcpSelected = enabledMcpIds.length === 0
    || (selectableMcpIds.length > 0 && sanitizedEnabledMcpIds.length === selectableMcpIds.length);
  const selectedMcpCount = isAllMcpSelected ? selectableMcpIds.length : sanitizedEnabledMcpIds.length;
  const { builtinMcpConfigs, customMcpConfigs } = useMemo(
    () => splitMcpConfigsByBuiltin(availableMcpConfigs),
    [availableMcpConfigs],
  );
  const mcpToolsetPresets = useMemo(
    () => buildMcpToolsetPresets(selectableMcpIds, availableMcpIds),
    [availableMcpIds, selectableMcpIds],
  );
  const projectMcpDefault = useMemo(() => {
    if (!normalizedProjectScopeKey) {
      return null;
    }
    return readProjectDefaultMap()[normalizedProjectScopeKey] || null;
  }, [normalizedProjectScopeKey, projectDefaultsVersion]);
  const hasProjectMcpDefault = !!projectMcpDefault;

  const loadAvailableMcpConfigs = useCallback(async (options?: { forceRefresh?: boolean }) => {
    setMcpConfigsLoading(true);
    setMcpConfigsError(null);
    try {
      const rows = await client.getMcpConfigs(undefined, {
        forceRefresh: options?.forceRefresh === true,
      });
      setAvailableMcpConfigs(normalizeSelectableMcpConfigs(rows));
    } catch (error) {
      setMcpConfigsError(error instanceof Error ? error.message : '加载 MCP 列表失败');
      setAvailableMcpConfigs([]);
    } finally {
      setMcpConfigsLoading(false);
    }
  }, [client]);

  useEffect(() => {
    if (!mcpEnabled) {
      return;
    }
    if (availableMcpConfigs.length > 0 || mcpConfigsLoading) {
      return;
    }
    void loadAvailableMcpConfigs();
  }, [availableMcpConfigs.length, loadAvailableMcpConfigs, mcpConfigsLoading, mcpEnabled]);

  useEffect(() => {
    if (!mcpPickerOpen) {
      return;
    }
    if (availableMcpConfigs.length > 0 || mcpConfigsLoading) {
      return;
    }
    void loadAvailableMcpConfigs();
  }, [availableMcpConfigs.length, loadAvailableMcpConfigs, mcpConfigsLoading, mcpPickerOpen]);

  useEffect(() => {
    if (!onEnabledMcpIdsChange) {
      return;
    }
    if (availableMcpIds.length === 0) {
      return;
    }
    const sameLength = enabledMcpIds.length === sanitizedEnabledMcpIds.length;
    const sameValues = sameLength && enabledMcpIds.every((id, index) => id === sanitizedEnabledMcpIds[index]);
    if (sameValues) {
      return;
    }
    onEnabledMcpIdsChange(sanitizedEnabledMcpIds);
  }, [
    availableMcpIds.length,
    enabledMcpIds,
    onEnabledMcpIdsChange,
    sanitizedEnabledMcpIds,
  ]);

  const handleToggleMcpPicker = useCallback(() => {
    if (disabled || isStreaming || isStopping) return;
    setMcpPickerOpen((prev) => !prev);
  }, [disabled, isStopping, isStreaming]);

  const applySelectedMcpIds = useCallback((ids: string[]) => {
    if (!onEnabledMcpIdsChange) {
      return;
    }
    const uniqueIds: string[] = [];
    for (const id of ids) {
      const trimmed = id.trim();
      if (!trimmed || uniqueIds.includes(trimmed)) {
        continue;
      }
      uniqueIds.push(trimmed);
    }
    if (allowAllShortcut && uniqueIds.length === selectableMcpIds.length) {
      onEnabledMcpIdsChange([]);
      return;
    }
    onEnabledMcpIdsChange(uniqueIds);
  }, [allowAllShortcut, onEnabledMcpIdsChange, selectableMcpIds.length]);

  const handleSelectAllMcp = useCallback(() => {
    if (!onEnabledMcpIdsChange) {
      return;
    }
    if (allowAllShortcut) {
      onEnabledMcpIdsChange([]);
      return;
    }
    onEnabledMcpIdsChange([...selectableMcpIds]);
  }, [allowAllShortcut, onEnabledMcpIdsChange, selectableMcpIds]);

  const handleToggleMcpSelection = useCallback((mcpId: string) => {
    if (!onEnabledMcpIdsChange) {
      return;
    }
    if (!selectableMcpIdSet.has(mcpId)) {
      return;
    }
    const baseSelected = isAllMcpSelected ? [...selectableMcpIds] : [...sanitizedEnabledMcpIds];
    const exists = baseSelected.includes(mcpId);
    const nextSelected = exists
      ? baseSelected.filter((id) => id !== mcpId)
      : [...baseSelected, mcpId];
    if (nextSelected.length === 0) {
      onMcpEnabledChange?.(false);
      onEnabledMcpIdsChange([]);
      return;
    }
    applySelectedMcpIds(nextSelected);
  }, [
    applySelectedMcpIds,
    isAllMcpSelected,
    onEnabledMcpIdsChange,
    onMcpEnabledChange,
    sanitizedEnabledMcpIds,
    selectableMcpIdSet,
    selectableMcpIds,
  ]);

  const handleApplyMcpToolsetPreset = useCallback((presetId: string) => {
    if (!onEnabledMcpIdsChange) {
      return;
    }
    const preset = mcpToolsetPresets.find((item) => item.id === presetId);
    if (!preset || preset.disabled) {
      return;
    }
    if (!mcpEnabled) {
      onMcpEnabledChange?.(true);
    }
    applySelectedMcpIds(preset.targetIds);
  }, [
    applySelectedMcpIds,
    mcpEnabled,
    mcpToolsetPresets,
    onEnabledMcpIdsChange,
    onMcpEnabledChange,
  ]);

  const handleSaveProjectMcpDefault = useCallback(() => {
    if (!normalizedProjectScopeKey) {
      return;
    }
    const explicitSelection = isAllMcpSelected
      ? [...selectableMcpIds]
      : [...sanitizedEnabledMcpIds];
    const nextMap = readProjectDefaultMap();
    nextMap[normalizedProjectScopeKey] = {
      mcpEnabled,
      enabledMcpIds: normalizeEnabledMcpIdList(explicitSelection),
      updatedAt: new Date().toISOString(),
    };
    writeProjectDefaultMap(nextMap);
    setProjectDefaultsVersion((prev) => prev + 1);
  }, [
    isAllMcpSelected,
    mcpEnabled,
    normalizedProjectScopeKey,
    sanitizedEnabledMcpIds,
    selectableMcpIds,
  ]);

  const handleApplyProjectMcpDefault = useCallback(() => {
    if (!projectMcpDefault) {
      return;
    }
    if (!projectMcpDefault.mcpEnabled) {
      onMcpEnabledChange?.(false);
      onEnabledMcpIdsChange?.([]);
      return;
    }

    const filtered = projectMcpDefault.enabledMcpIds.filter((id) => selectableMcpIdSet.has(id));
    if (filtered.length === 0) {
      onMcpEnabledChange?.(false);
      onEnabledMcpIdsChange?.([]);
      return;
    }
    if (!mcpEnabled) {
      onMcpEnabledChange?.(true);
    }
    applySelectedMcpIds(filtered);
  }, [
    applySelectedMcpIds,
    mcpEnabled,
    onEnabledMcpIdsChange,
    onMcpEnabledChange,
    projectMcpDefault,
    selectableMcpIdSet,
  ]);

  const isProjectRequiredMcpId = useCallback((id: string) => PROJECT_REQUIRED_MCP_IDS.has(id), []);
  const isRemoteRequiredMcpId = useCallback((id: string) => REMOTE_REQUIRED_MCP_IDS.has(id), []);

  return {
    mcpPickerOpen,
    setMcpPickerOpen,
    availableMcpConfigs,
    mcpConfigsLoading,
    mcpConfigsError,
    builtinMcpConfigs,
    customMcpConfigs,
    mcpToolsetPresets,
    projectScopeKey: normalizedProjectScopeKey,
    hasProjectMcpDefault,
    selectableMcpIds,
    sanitizedEnabledMcpIds,
    isAllMcpSelected,
    selectedMcpCount,
    isProjectRequiredMcpId,
    isRemoteRequiredMcpId,
    loadAvailableMcpConfigs,
    handleToggleMcpPicker,
    handleSelectAllMcp,
    handleToggleMcpSelection,
    handleApplyMcpToolsetPreset,
    handleSaveProjectMcpDefault,
    handleApplyProjectMcpDefault,
  };
};
