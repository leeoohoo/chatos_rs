// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { TranslateFn } from '../i18n/I18nProvider';

export type ThinkingOption = {
  value: string;
  label: string;
};

export const normalizeThinkingProvider = (provider: string | undefined): string => (
  (provider || 'gpt').trim().toLowerCase()
);

export const thinkingOptionsForProvider = (
  provider: string | undefined,
  t: TranslateFn,
): ThinkingOption[] => {
  const normalized = normalizeThinkingProvider(provider);
  if (normalized === 'deepseek') {
    return [
      { value: '', label: t('inputArea.model.thinking.default') },
      { value: 'none', label: t('inputArea.model.thinking.off') },
      { value: 'high', label: t('inputArea.model.thinking.value', { value: 'high' }) },
      { value: 'max', label: t('inputArea.model.thinking.value', { value: 'max' }) },
    ];
  }
  if (normalized === 'kimi' || normalized === 'kimik2' || normalized === 'moonshot') {
    return [
      { value: '', label: t('inputArea.model.thinking.default') },
      { value: 'auto', label: t('inputArea.model.thinking.auto') },
      { value: 'none', label: t('inputArea.model.thinking.off') },
    ];
  }
  if (
    normalized === 'glm'
    || normalized === 'zhipu'
    || normalized === 'zai'
  ) {
    return [
      { value: '', label: t('inputArea.model.thinking.default') },
      { value: 'none', label: t('inputArea.model.thinking.value', { value: 'none' }) },
      { value: 'low', label: t('inputArea.model.thinking.value', { value: 'low' }) },
      { value: 'medium', label: t('inputArea.model.thinking.value', { value: 'medium' }) },
      { value: 'high', label: t('inputArea.model.thinking.value', { value: 'high' }) },
      { value: 'xhigh', label: t('inputArea.model.thinking.value', { value: 'xhigh' }) },
    ];
  }
  return [
    { value: '', label: t('inputArea.model.thinking.default') },
    { value: 'none', label: t('inputArea.model.thinking.value', { value: 'none' }) },
    { value: 'minimal', label: t('inputArea.model.thinking.value', { value: 'minimal' }) },
    { value: 'low', label: t('inputArea.model.thinking.value', { value: 'low' }) },
    { value: 'medium', label: t('inputArea.model.thinking.value', { value: 'medium' }) },
    { value: 'high', label: t('inputArea.model.thinking.value', { value: 'high' }) },
    { value: 'xhigh', label: t('inputArea.model.thinking.value', { value: 'xhigh' }) },
  ];
};
