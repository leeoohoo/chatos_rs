import type {
  AgentMemoryJobConfig,
  RollupJobConfig,
  SummaryJobConfig,
  TaskExecutionRollupJobConfig,
  TaskExecutionSummaryJobConfig,
} from '../../types';
import summaryPromptDefaults from '../../../../shared/summary_prompt_defaults.json';

type SummaryPromptDefaults = {
  summary: string;
  rollup: string;
  task_execution_summary: string;
  task_execution_rollup: string;
  agent_memory: string;
};

const DEFAULTS = summaryPromptDefaults as SummaryPromptDefaults;

export const DEFAULT_SUMMARY_PROMPT_TEMPLATE = DEFAULTS.summary;
export const DEFAULT_ROLLUP_PROMPT_TEMPLATE = DEFAULTS.rollup;
export const DEFAULT_TASK_EXECUTION_SUMMARY_PROMPT_TEMPLATE = DEFAULTS.task_execution_summary;
export const DEFAULT_TASK_EXECUTION_ROLLUP_PROMPT_TEMPLATE = DEFAULTS.task_execution_rollup;
export const DEFAULT_AGENT_MEMORY_PROMPT_TEMPLATE = DEFAULTS.agent_memory;

export const createSummaryConfig = (uid: string): SummaryJobConfig => ({
  user_id: uid,
  enabled: 1,
  summary_model_config_id: null,
  summary_prompt: DEFAULT_SUMMARY_PROMPT_TEMPLATE,
  token_limit: 6000,
  round_limit: 8,
  target_summary_tokens: 700,
  job_interval_seconds: 30,
  max_sessions_per_tick: 50,
});

export const createRollupConfig = (uid: string): RollupJobConfig => ({
  user_id: uid,
  enabled: 1,
  summary_model_config_id: null,
  summary_prompt: DEFAULT_ROLLUP_PROMPT_TEMPLATE,
  token_limit: 6000,
  round_limit: 50,
  target_summary_tokens: 700,
  job_interval_seconds: 60,
  keep_raw_level0_count: 0,
  max_level: 4,
  max_sessions_per_tick: 50,
});

export const createAgentMemoryConfig = (uid: string): AgentMemoryJobConfig => ({
  user_id: uid,
  enabled: 1,
  summary_model_config_id: null,
  summary_prompt: DEFAULT_AGENT_MEMORY_PROMPT_TEMPLATE,
  token_limit: 6000,
  round_limit: 20,
  target_summary_tokens: 700,
  job_interval_seconds: 60,
  keep_raw_level0_count: 0,
  max_level: 4,
  max_agents_per_tick: 50,
});

export const createTaskExecutionSummaryConfig = (
  uid: string,
): TaskExecutionSummaryJobConfig => ({
  user_id: uid,
  enabled: 1,
  summary_model_config_id: null,
  summary_prompt: DEFAULT_TASK_EXECUTION_SUMMARY_PROMPT_TEMPLATE,
  token_limit: 6000,
  round_limit: 8,
  target_summary_tokens: 700,
  job_interval_seconds: 30,
  max_scopes_per_tick: 50,
});

export const createTaskExecutionRollupConfig = (
  uid: string,
): TaskExecutionRollupJobConfig => ({
  user_id: uid,
  enabled: 1,
  summary_model_config_id: null,
  summary_prompt: DEFAULT_TASK_EXECUTION_ROLLUP_PROMPT_TEMPLATE,
  token_limit: 6000,
  round_limit: 50,
  target_summary_tokens: 700,
  job_interval_seconds: 60,
  keep_raw_level0_count: 0,
  max_level: 4,
  max_scopes_per_tick: 50,
});

export const normalizeMinInteger = (value: number | null, min: number): number => {
  if (value === null) {
    return min;
  }
  return Math.max(min, Math.floor(value));
};
