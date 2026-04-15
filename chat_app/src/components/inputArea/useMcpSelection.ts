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
const MCP_PROJECT_DEFAULTS_STORAGE_KEY = 'chatos_mcp_project_defaults_v1';

export interface SelectableMcpConfig {
  id: string;
  name: string;
  displayName: string;
  builtin: boolean;
}

interface McpToolsetPresetSpec {
  id: string;
  label: string;
  description: string;
  preferredIds: string[];
}

export interface McpToolsetPreset {
  id: string;
  label: string;
  description: string;
  targetIds: string[];
  disabled: boolean;
}

const MCP_TOOLSET_PRESET_SPECS: McpToolsetPresetSpec[] = [
  {
    id: 'coding',
    label: '代码开发',
    description: '代码读写 + 终端 + 任务，适合实现与调试',
    preferredIds: [
      'builtin_code_maintainer_read',
      'builtin_code_maintainer_write',
      'builtin_code_maintainer',
      'builtin_terminal_controller',
      'builtin_task_manager',
      'builtin_notepad',
    ],
  },
  {
    id: 'web_research',
    label: 'Web 研究',
    description: '网页搜索/提取 + 浏览器自动化 + 只读代码',
    preferredIds: [
      'builtin_web_tools',
      'builtin_browser_tools',
      'builtin_code_maintainer_read',
      'builtin_notepad',
    ],
  },
  {
    id: 'remote_ops',
    label: '远程运维',
    description: '远程连接 + 终端 + 任务，适合服务器排障',
    preferredIds: [
      'builtin_remote_connection_controller',
      'builtin_terminal_controller',
      'builtin_task_manager',
      'builtin_code_maintainer_read',
    ],
  },
  {
    id: 'minimal',
    label: '轻量模式',
    description: '仅保留最小必要工具，减少噪音',
    preferredIds: [
      'builtin_code_maintainer_read',
      'builtin_terminal_controller',
    ],
  },
];

export function buildMcpToolsetPresets(
  selectableMcpIds: string[],
  availableMcpIds: string[],
): McpToolsetPreset[] {
  const selectableSet = new Set(selectableMcpIds);
  const availableSet = new Set(availableMcpIds);
  return MCP_TOOLSET_PRESET_SPECS.map((preset) => {
    const targetIds: string[] = [];
    for (const candidateId of preset.preferredIds) {
      if (!availableSet.has(candidateId) || !selectableSet.has(candidateId)) {
        continue;
      }
      if (!targetIds.includes(candidateId)) {
        targetIds.push(candidateId);
      }
    }
    return {
      id: preset.id,
      label: preset.label,
      description: preset.description,
      targetIds,
      disabled: targetIds.length === 0,
    };
  });
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

interface StoredMcpProjectDefault {
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  updatedAt: string;
}

type StoredMcpProjectDefaultMap = Record<string, StoredMcpProjectDefault>;

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

const normalizeProjectScopeKey = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

const normalizeEnabledMcpIdList = (value: unknown): string[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  const out: string[] = [];
  for (const item of value) {
    if (typeof item !== 'string') {
      continue;
    }
    const trimmed = item.trim();
    if (!trimmed || out.includes(trimmed)) {
      continue;
    }
    out.push(trimmed);
  }
  return out;
};

const readProjectDefaultMap = (): StoredMcpProjectDefaultMap => {
  if (typeof window === 'undefined' || !window.localStorage) {
    return {};
  }
  try {
    const raw = window.localStorage.getItem(MCP_PROJECT_DEFAULTS_STORAGE_KEY);
    if (!raw) {
      return {};
    }
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return {};
    }

    const out: StoredMcpProjectDefaultMap = {};
    for (const [key, value] of Object.entries(parsed as Record<string, unknown>)) {
      const normalizedKey = normalizeProjectScopeKey(key);
      if (!normalizedKey || !value || typeof value !== 'object' || Array.isArray(value)) {
        continue;
      }
      const entry = value as Record<string, unknown>;
      out[normalizedKey] = {
        mcpEnabled: entry.mcpEnabled !== false,
        enabledMcpIds: normalizeEnabledMcpIdList(entry.enabledMcpIds),
        updatedAt: typeof entry.updatedAt === 'string' ? entry.updatedAt : '',
      };
    }
    return out;
  } catch {
    return {};
  }
};

const writeProjectDefaultMap = (value: StoredMcpProjectDefaultMap): void => {
  if (typeof window === 'undefined' || !window.localStorage) {
    return;
  }
  try {
    window.localStorage.setItem(MCP_PROJECT_DEFAULTS_STORAGE_KEY, JSON.stringify(value));
  } catch {
    // ignore localStorage write errors
  }
};

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
