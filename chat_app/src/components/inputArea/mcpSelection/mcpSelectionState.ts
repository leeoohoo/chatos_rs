import {
  PROJECT_REQUIRED_MCP_IDS,
  REMOTE_REQUIRED_MCP_IDS,
} from './mcpSelectionConstants';
import type { SelectableMcpConfig } from './mcpSelectionTypes';

export const buildSelectableMcpIds = (
  availableMcpIds: string[],
  hasDirectoryContext: boolean,
  hasRemoteContext: boolean,
): string[] => availableMcpIds.filter((id) => {
  if (!hasDirectoryContext && PROJECT_REQUIRED_MCP_IDS.has(id)) {
    return false;
  }
  if (!hasRemoteContext && REMOTE_REQUIRED_MCP_IDS.has(id)) {
    return false;
  }
  return true;
});

export const sanitizeEnabledMcpIds = ({
  availableMcpIds,
  enabledMcpIds,
  selectableMcpIds,
  selectableMcpIdSet,
  allowAllShortcut,
}: {
  availableMcpIds: string[];
  enabledMcpIds: string[];
  selectableMcpIds: string[];
  selectableMcpIdSet: Set<string>;
  allowAllShortcut: boolean;
}): string[] => {
  if (availableMcpIds.length === 0) {
    return enabledMcpIds;
  }
  if (enabledMcpIds.length === 0) {
    return allowAllShortcut ? [] : [...selectableMcpIds];
  }
  return enabledMcpIds.filter((id) => selectableMcpIdSet.has(id));
};

export const splitMcpConfigsByBuiltin = (
  configs: SelectableMcpConfig[],
): {
  builtinMcpConfigs: SelectableMcpConfig[];
  customMcpConfigs: SelectableMcpConfig[];
} => ({
  builtinMcpConfigs: configs.filter((item) => item.builtin),
  customMcpConfigs: configs.filter((item) => !item.builtin),
});
