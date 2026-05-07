import type { SessionSummaryJobConfigResponse } from '../../lib/api/client/types';

export interface SummaryJobConfigForm {
  enabled: boolean;
  summary_model_config_id: string;
  token_limit: number;
  message_count_limit: number;
  target_summary_tokens: number;
  job_interval_seconds: number;
}

export interface RangeLimit {
  min: number;
  max?: number;
}

export interface SummaryJobLimits {
  token_limit: RangeLimit;
  message_count_limit: RangeLimit;
  target_summary_tokens: RangeLimit;
  job_interval_seconds: RangeLimit;
}

export const DEFAULT_SUMMARY_FORM: SummaryJobConfigForm = {
  enabled: true,
  summary_model_config_id: '',
  token_limit: 6000,
  message_count_limit: 8,
  target_summary_tokens: 700,
  job_interval_seconds: 30,
};

export const DEFAULT_SUMMARY_LIMITS: SummaryJobLimits = {
  token_limit: { min: 500 },
  message_count_limit: { min: 1 },
  target_summary_tokens: { min: 200 },
  job_interval_seconds: { min: 10 },
};

type RangeLimitSource = {
  min?: unknown;
  max?: unknown;
};

type SummaryLimitsSource = {
  token_limit?: unknown;
  message_count_limit?: unknown;
  round_limit?: unknown;
  target_summary_tokens?: unknown;
  job_interval_seconds?: unknown;
};

type SummaryConfigWithLimits = SessionSummaryJobConfigResponse & {
  limits?: SummaryLimitsSource | null;
};

const asRangeLimitSource = (value: unknown): RangeLimitSource | null => (
  value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as RangeLimitSource
    : null
);

const asSummaryConfigWithLimits = (value: unknown): SummaryConfigWithLimits => (
  value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as SummaryConfigWithLimits
    : {}
);

export const getErrorMessage = (error: unknown): string => {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  if (error !== null && typeof error === 'object' && 'message' in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === 'string' && message.trim()) {
      return message;
    }
  }
  return String(error);
};

export const clampNumber = (value: number, range: RangeLimit): number => {
  if (!Number.isFinite(value)) {
    return range.min;
  }
  if (Number.isFinite(range.max)) {
    return Math.max(range.min, Math.min(range.max as number, value));
  }
  return Math.max(range.min, value);
};

export const rangeText = (range: RangeLimit): string => {
  if (Number.isFinite(range.max)) {
    return `${range.min}-${range.max}`;
  }
  return `>=${range.min}`;
};

export const parseRangeLimit = (input: unknown, fallback: RangeLimit): RangeLimit => {
  const source = asRangeLimitSource(input);
  const min = Number(source?.min);
  const max = Number(source?.max);
  if (Number.isFinite(min) && Number.isFinite(max) && max >= min) {
    return { min, max };
  }
  if (Number.isFinite(min)) {
    return { min };
  }
  return fallback;
};

export const parseSummaryLimits = (config: unknown): SummaryJobLimits => {
  const source = asSummaryConfigWithLimits(config);
  const limits = source.limits || {};
  return {
    token_limit: parseRangeLimit(limits.token_limit, DEFAULT_SUMMARY_LIMITS.token_limit),
    message_count_limit: parseRangeLimit(
      limits.message_count_limit || limits.round_limit,
      DEFAULT_SUMMARY_LIMITS.message_count_limit,
    ),
    target_summary_tokens: parseRangeLimit(
      limits.target_summary_tokens,
      DEFAULT_SUMMARY_LIMITS.target_summary_tokens,
    ),
    job_interval_seconds: parseRangeLimit(
      limits.job_interval_seconds,
      DEFAULT_SUMMARY_LIMITS.job_interval_seconds,
    ),
  };
};

export const buildSummaryForm = (
  config: unknown,
  limits: SummaryJobLimits,
  fallback: SummaryJobConfigForm = DEFAULT_SUMMARY_FORM,
): SummaryJobConfigForm => {
  const source = asSummaryConfigWithLimits(config);
  return {
    enabled: source.enabled !== false,
    summary_model_config_id: String(source.summary_model_config_id || ''),
    token_limit: clampNumber(
      Number(source.token_limit || fallback.token_limit),
      limits.token_limit,
    ),
    message_count_limit: clampNumber(
      Number(source.message_count_limit || source.round_limit || fallback.message_count_limit),
      limits.message_count_limit,
    ),
    target_summary_tokens: clampNumber(
      Number(source.target_summary_tokens || fallback.target_summary_tokens),
      limits.target_summary_tokens,
    ),
    job_interval_seconds: clampNumber(
      Number(source.job_interval_seconds || fallback.job_interval_seconds),
      limits.job_interval_seconds,
    ),
  };
};
