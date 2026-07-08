// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { AiModelConfig } from '../../types';

export const formatSummaryCreatedAt = (value: string): string => {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value || '-';
  }
  return parsed.toLocaleString('zh-CN', { hour12: false });
};

export const buildSupportedFileTypes = (supportsImages: boolean): string[] => (
  supportsImages
    ? ['image/*', 'text/*', 'application/json', 'application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']
    : ['text/*', 'application/json', 'application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']
);

export const resolveModelSupportFlags = (
  selectedModelId: string | null,
  aiModelConfigs: AiModelConfig[],
): { supportsImages: boolean; supportsReasoning: boolean } => {
  if (!selectedModelId) {
    return { supportsImages: false, supportsReasoning: false };
  }
  const matched = (aiModelConfigs || []).find((item) => item?.id === selectedModelId);
  return {
    supportsImages: matched?.supports_images === true,
    supportsReasoning: matched?.supports_reasoning === true,
  };
};
