import type {
  EngineJobPolicy,
  EngineJobRun,
  EngineModelProfile,
  EngineSource,
  ThreadQuery,
  UpsertEngineJobPolicyPayload,
  UpsertEngineModelProfilePayload,
  UpsertEngineSourcePayload,
} from '../types';

export type TabKey = 'dashboard' | 'data' | 'sources' | 'models' | 'policies' | 'runs';

export type ModelFormValues = {
  name: string;
  provider: string;
  model: string;
  base_url?: string;
  api_key?: string;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  temperature?: number | null;
  thinking_level?: string;
  is_default: boolean;
  enabled: boolean;
};

export type PolicyFormValues = {
  enabled: boolean;
  model_profile_id?: string;
  summary_prompt?: string;
  summary_prompt_zh?: string;
  summary_prompt_en?: string;
  summary_prompt_language?: 'zh' | 'en';
  rollup_summary_prompt?: string;
  rollup_summary_prompt_zh?: string;
  rollup_summary_prompt_en?: string;
  rollup_summary_prompt_language?: 'zh' | 'en';
  token_limit?: number | null;
  target_summary_tokens?: number | null;
  interval_seconds?: number | null;
  max_threads_per_tick?: number | null;
  count_limit?: number | null;
  keep_level0_count?: number | null;
  max_level?: number | null;
};

export type SourceFormValues = {
  tenant_id: string;
  source_id: string;
  name: string;
  description?: string;
  enabled: boolean;
};

export type ThreadFilterFormValues = ThreadQuery;
export type JobTypeKey = 'summary' | 'rollup' | 'subject_memory' | 'thread_repair';
export type PolicyViewKey =
  | 'summary'
  | 'rollup'
  | 'memory_from_summary'
  | 'memory_rollup'
  | 'thread_repair';

export type PolicyMeta = {
  tabLabel: string;
  title: string;
  tagColor: string;
  description: string;
  inputText: string;
  outputText: string;
  purposeText: string;
  promptLabel: string;
  promptPlaceholder?: string;
  tokenLimitLabel: string;
  targetSummaryTokensLabel: string;
  showTargetSummaryTokens?: boolean;
  intervalSecondsLabel: string;
  maxThreadsPerTickLabel?: string;
  showMaxThreadsPerTick?: boolean;
  countLimitLabel?: string;
  keepLevel0Label?: string;
  maxLevelLabel?: string;
  sharedPolicyHint?: string;
  showKeepLevel0: boolean;
  showMaxLevel: boolean;
};

export type ToolSection = {
  key: string;
  label: string;
  body: string;
};

export type DashboardStats = {
  sources: number;
  models: number;
  policies: number;
  running: number;
  done: number;
  failed: number;
};

export type PolicyMap = Partial<Record<JobTypeKey, EngineJobPolicy>>;

export type SourcePayloadResult = {
  sourceId: string;
  payload: UpsertEngineSourcePayload;
};

export type ModelOptions = Array<{ label: string; value: string }>;

export type ThreadNameLookup = Record<string, string>;

export type PolicySaveHandler = (
  jobType: string,
  values: PolicyFormValues,
) => Promise<void>;

export type PolicyPromptGenerator = (
  jobType: string,
  promptField: 'summary_prompt' | 'rollup_summary_prompt',
  userInput: string,
) => Promise<{ prompt_zh: string; prompt_en: string }>;

export type BuildModelPayload = UpsertEngineModelProfilePayload;
export type BuildPolicyPayload = UpsertEngineJobPolicyPayload;

export type ThreadRunLookupInput = Pick<
  EngineJobRun,
  'thread_id' | 'tenant_id' | 'source_id'
>;

export type ThreadNameInput = Pick<EngineModelProfile, never> | null;

export type ThreadDisplayInput = Pick<EngineSource, never> | null;
