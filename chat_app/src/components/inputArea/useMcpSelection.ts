import { useCallback, useEffect, useMemo, useState, type Dispatch, type SetStateAction } from 'react';

const AGENT_BUILDER_MCP_ID = 'builtin_agent_builder';
const PROJECT_REQUIRED_MCP_IDS = new Set([
  'builtin_code_maintainer',
  'builtin_code_maintainer_read',
  'builtin_code_maintainer_write',
  'builtin_terminal_controller',
]);
const REMOTE_REQUIRED_MCP_IDS = new Set([
  'builtin_remote_connection_controller',
]);

export interface SelectableMcpConfig {
  id: string;
  name: string;
  displayName: string;
  builtin: boolean;
}

interface McpConfigLike {
  id?: string;
  name?: string;
  display_name?: string;
  enabled?: boolean;
  builtin?: boolean;
}

interface McpApiClient {
  getMcpConfigs: () => Promise<unknown>;
}

interface UseMcpSelectionOptions {
  client: McpApiClient;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
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
}

export const useMcpSelection = ({
  client,
  mcpEnabled,
  enabledMcpIds,
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

  const availableMcpIds = useMemo(
    () => availableMcpConfigs.map((item) => item.id),
    [availableMcpConfigs],
  );
  const selectableMcpIds = useMemo(
    () => availableMcpIds.filter((id) => {
      if (!hasDirectoryContext && PROJECT_REQUIRED_MCP_IDS.has(id)) {
        return false;
      }
      if (!hasRemoteContext && REMOTE_REQUIRED_MCP_IDS.has(id)) {
        return false;
      }
      return true;
    }),
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
  const sanitizedEnabledMcpIds = useMemo(() => {
    if (availableMcpIds.length === 0) {
      return enabledMcpIds;
    }
    if (enabledMcpIds.length === 0) {
      return allowAllShortcut ? [] : [...selectableMcpIds];
    }
    return enabledMcpIds.filter((id) => selectableMcpIdSet.has(id));
  }, [
    allowAllShortcut,
    availableMcpIds.length,
    enabledMcpIds,
    selectableMcpIdSet,
    selectableMcpIds,
  ]);
  const isAllMcpSelected = enabledMcpIds.length === 0
    || (selectableMcpIds.length > 0 && sanitizedEnabledMcpIds.length === selectableMcpIds.length);
  const selectedMcpCount = isAllMcpSelected ? selectableMcpIds.length : sanitizedEnabledMcpIds.length;
  const builtinMcpConfigs = useMemo(
    () => availableMcpConfigs.filter((item) => item.builtin),
    [availableMcpConfigs],
  );
  const customMcpConfigs = useMemo(
    () => availableMcpConfigs.filter((item) => !item.builtin),
    [availableMcpConfigs],
  );

  const loadAvailableMcpConfigs = useCallback(async () => {
    setMcpConfigsLoading(true);
    setMcpConfigsError(null);
    try {
      const rows = await client.getMcpConfigs();
      const seenIds = new Set<string>();
      const normalized = (Array.isArray(rows) ? rows : [])
        .map((item) => {
          const candidate = (item && typeof item === 'object' ? item : {}) as McpConfigLike;
          const id = typeof candidate.id === 'string' ? candidate.id.trim() : '';
          if (!id || id === AGENT_BUILDER_MCP_ID) {
            return null;
          }
          if (seenIds.has(id)) {
            return null;
          }
          seenIds.add(id);
          const enabled = typeof candidate.enabled === 'boolean' ? candidate.enabled : true;
          if (!enabled) {
            return null;
          }
          const displayNameRaw = typeof candidate.display_name === 'string' ? candidate.display_name.trim() : '';
          const nameRaw = typeof candidate.name === 'string' ? candidate.name.trim() : '';
          return {
            id,
            name: nameRaw || id,
            displayName: displayNameRaw || nameRaw || id,
            builtin: candidate.builtin === true,
          } satisfies SelectableMcpConfig;
        })
        .filter((item: SelectableMcpConfig | null): item is SelectableMcpConfig => item !== null)
        .sort((left, right) => {
          if (left.builtin !== right.builtin) {
            return left.builtin ? -1 : 1;
          }
          return left.displayName.localeCompare(right.displayName, 'zh-Hans-CN');
        });

      setAvailableMcpConfigs(normalized);
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
  };
};
