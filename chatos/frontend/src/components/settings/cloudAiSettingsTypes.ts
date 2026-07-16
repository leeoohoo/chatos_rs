// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type DefaultModelSlot = 'memory' | 'project' | 'environment';

export interface DefaultModelDraft {
  modelId: string;
  thinking: string;
}

export type DefaultModelDrafts = Record<DefaultModelSlot, DefaultModelDraft>;

export interface TaskModelDraft {
  usage: string;
  thinking: string;
  temperature: string;
  maxOutputTokens: string;
  enabled: boolean;
}

export type TaskModelDrafts = Record<string, TaskModelDraft>;

export const emptyDefaultModelDrafts = (): DefaultModelDrafts => ({
  memory: { modelId: '', thinking: '' },
  project: { modelId: '', thinking: '' },
  environment: { modelId: '', thinking: '' },
});
