export interface SelectableMcpConfig {
  id: string;
  name: string;
  displayName: string;
  builtin: boolean;
}

export interface McpToolsetPresetSpec {
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

export interface StoredMcpProjectDefault {
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  updatedAt: string;
}

export type StoredMcpProjectDefaultMap = Record<string, StoredMcpProjectDefault>;
