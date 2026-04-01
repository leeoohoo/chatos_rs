import type { SystemContext } from '../../types';

export type ViewMode = 'list' | 'create' | 'edit';

export interface PromptQualityReport {
  clarity?: number;
  constraint_completeness?: number;
  conflict_risk?: number;
  verbosity?: number;
  overall?: number;
  warnings?: string[];
}

export interface PromptCandidate {
  title?: string;
  content: string;
  score?: number;
  report?: PromptQualityReport;
}

export interface SystemContextFormData {
  name: string;
  content: string;
}

export interface AssistantFormState {
  scene: string;
  style: string;
  language: string;
  outputFormat: string;
  constraintsText: string;
  forbiddenText: string;
  optimizeGoal: string;
}

export type SystemContextLike = Partial<SystemContext> & {
  id?: string;
  name?: string;
  content?: string;
  updated_at?: string | Date;
  created_at?: string | Date;
  app_ids?: string[];
};

export type AiModelConfigLike = {
  id?: string;
  model_name?: string;
  model?: string;
  provider?: string;
  api_key?: string;
  base_url?: string;
};

export interface SystemContextDraftGenerateResponse {
  candidates?: PromptCandidate[];
}

export interface SystemContextDraftOptimizeResponse {
  optimized_content?: string;
  score_after?: number;
  report_after?: PromptQualityReport;
}

export interface SystemContextDraftEvaluateResponse {
  report?: PromptQualityReport;
}

export interface SystemContextEditorStoreLike {
  systemContexts?: SystemContextLike[];
  loadSystemContexts: () => Promise<void>;
  createSystemContext: (
    name: string,
    content: string,
    appIds?: string[],
  ) => Promise<SystemContextLike | null | undefined>;
  updateSystemContext: (
    id: string,
    name: string,
    content: string,
    appIds?: string[],
  ) => Promise<SystemContextLike | null | undefined>;
  deleteSystemContext: (id: string) => Promise<void>;
  generateSystemContextDraft?: (payload: {
    scene: string;
    style?: string;
    language?: string;
    output_format?: string;
    constraints?: string[];
    forbidden?: string[];
    candidate_count?: number;
    ai_model_config?: Record<string, unknown>;
  }) => Promise<SystemContextDraftGenerateResponse | null | undefined>;
  optimizeSystemContextDraft?: (payload: {
    content: string;
    goal?: string;
    keep_intent?: boolean;
    ai_model_config?: Record<string, unknown>;
  }) => Promise<SystemContextDraftOptimizeResponse | null | undefined>;
  evaluateSystemContextDraft?: (payload: {
    content: string;
  }) => Promise<SystemContextDraftEvaluateResponse | null | undefined>;
  aiModelConfigs?: AiModelConfigLike[];
  selectedModelId?: string | null;
}
