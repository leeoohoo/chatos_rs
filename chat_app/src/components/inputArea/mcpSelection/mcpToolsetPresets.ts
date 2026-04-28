import { MCP_TOOLSET_PRESET_SPECS } from './mcpSelectionConstants';
import type { McpToolsetPreset } from './mcpSelectionTypes';

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
