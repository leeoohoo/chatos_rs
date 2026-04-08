import type { FormEvent } from 'react';

import type { McpConfig } from '../../types';

export interface McpManagerProps {
  onClose?: () => void;
  store?: () => {
    mcpConfigs: McpConfig[];
    updateMcpConfig: (config: McpConfig) => Promise<McpConfig | null>;
    deleteMcpConfig: (id: string) => Promise<void>;
    loadMcpConfigs: () => Promise<void>;
  };
}

export interface McpFormData {
  name: string;
  command: string;
  type: 'http' | 'stdio';
  cwd?: string;
  argsInput?: string;
}

export type DynamicConfigValue = boolean | number | string | string[] | null;

export type DynamicConfigRecord = Record<string, DynamicConfigValue>;

export interface McpManagerFormProps {
  showAddForm: boolean;
  editingConfig: McpConfig | null;
  formData: McpFormData;
  dynamicConfig: DynamicConfigRecord;
  configLoading: boolean;
  configError: string | null;
  onCreate: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => Promise<void>;
  onCancel: () => void;
  onFormDataChange: (patch: Partial<McpFormData>) => void;
  onFetchDynamicConfig: () => Promise<void>;
  onDynamicConfigChange: (key: string, value: DynamicConfigValue) => void;
}

export interface McpServerListProps {
  mcpConfigs: McpConfig[];
  onEdit: (config: McpConfig) => void;
  onDelete: (id: string) => void;
}
