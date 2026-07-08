// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FormEvent } from 'react';

import type { AiModelConfig, AiModelProvider } from '../../types';

export interface AiModelManagerProps {
  onClose: () => void;
  store?: () => {
    aiModelConfigs: AiModelConfig[];
    loadAiModelConfigs: (options?: { force?: boolean }) => Promise<void>;
    updateAiModelConfig: (
      config: AiModelConfig,
      options?: { clearApiKey?: boolean },
    ) => Promise<void>;
    deleteAiModelConfig: (id: string) => Promise<void>;
  };
}

export interface AiModelFormData {
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  has_stored_api_key: boolean;
  clear_api_key: boolean;
  model_name: string;
  thinking_level: string;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
}

export interface AiModelManagerFormProps {
  editingConfig: AiModelProvider | null;
  formData: AiModelFormData;
  showTitle?: boolean;
  onSubmit: (event: FormEvent<HTMLFormElement>) => Promise<void>;
  onCancel: () => void;
  onFormDataChange: (patch: Partial<AiModelFormData>) => void;
  apiKeyVisible?: boolean;
  apiKeyLoading?: boolean;
  refreshingModels?: boolean;
  onToggleApiKeyVisible?: () => void;
  onRefreshModels?: () => void;
}

export interface AiModelListProps {
  aiModelConfigs: AiModelProvider[];
  onToggleEnabled: (config: AiModelProvider) => Promise<void>;
  onEdit: (config: AiModelProvider) => void;
  onDelete: (id: string) => Promise<void>;
}
