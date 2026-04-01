import type {
  AgentMemoryJobConfig,
  RollupJobConfig,
  SummaryJobConfig,
} from '../../types';

export const DEFAULT_SUMMARY_PROMPT_TEMPLATE =
  '你是 Memory Server 的总结引擎。请输出结构化简洁总结，重点保留事实、决策、风险、待办。目标长度约 {{target_tokens}} tokens。';

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
  summary_prompt: DEFAULT_SUMMARY_PROMPT_TEMPLATE,
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
  summary_prompt: DEFAULT_SUMMARY_PROMPT_TEMPLATE,
  token_limit: 6000,
  round_limit: 20,
  target_summary_tokens: 700,
  job_interval_seconds: 60,
  keep_raw_level0_count: 0,
  max_level: 4,
  max_agents_per_tick: 50,
});

export const normalizeMinInteger = (value: number | null, min: number): number => {
  if (value === null) {
    return min;
  }
  return Math.max(min, Math.floor(value));
};
