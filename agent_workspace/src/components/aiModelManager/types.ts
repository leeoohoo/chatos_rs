import type { FormEvent } from 'react';

import type { AiModelConfig } from '../../types';

export interface AiModelManagerProps {
  onClose: () => void;
  store?: () => {
    aiModelConfigs: AiModelConfig[];
    loadAiModelConfigs: () => Promise<void>;
    updateAiModelConfig: (config: AiModelConfig) => Promise<void>;
    deleteAiModelConfig: (id: string) => Promise<void>;
  };
}

export interface AiModelFormData {
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  model_name: string;
  thinking_level: string;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
}

export interface AiModelManagerFormProps {
  showAddForm: boolean;
  editingConfig: AiModelConfig | null;
  formData: AiModelFormData;
  onCreate: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => Promise<void>;
  onCancel: () => void;
  onFormDataChange: (patch: Partial<AiModelFormData>) => void;
}

export interface AiModelListProps {
  aiModelConfigs: AiModelConfig[];
  onToggleEnabled: (config: AiModelConfig) => Promise<void>;
  onEdit: (config: AiModelConfig) => void;
  onDelete: (id: string) => Promise<void>;
}
