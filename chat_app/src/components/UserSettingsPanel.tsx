import React from 'react';
import {
  useChatApiClientFromContext,
  useChatRuntimeEnv,
  useChatStoreFromContext,
} from '../lib/store/ChatStoreContext';
import { apiClient as globalApiClient } from '../lib/api/client';

interface Props { onClose: () => void }

interface SummaryJobConfigForm {
  enabled: boolean;
  summary_model_config_id: string;
  token_limit: number;
  message_count_limit: number;
  target_summary_tokens: number;
  job_interval_seconds: number;
}

interface TaskExecutionSummaryJobConfigForm {
  enabled: boolean;
  summary_model_config_id: string;
  token_limit: number;
  round_limit: number;
  target_summary_tokens: number;
  job_interval_seconds: number;
  max_scopes_per_tick: number;
}

interface TaskExecutionRollupJobConfigForm {
  enabled: boolean;
  summary_model_config_id: string;
  token_limit: number;
  round_limit: number;
  target_summary_tokens: number;
  job_interval_seconds: number;
  keep_raw_level0_count: number;
  max_level: number;
  max_scopes_per_tick: number;
}

interface RangeLimit {
  min: number;
  max?: number;
}

interface SummaryJobLimits {
  token_limit: RangeLimit;
  message_count_limit: RangeLimit;
  target_summary_tokens: RangeLimit;
  job_interval_seconds: RangeLimit;
}

interface TaskExecutionSummaryLimits {
  token_limit: RangeLimit;
  round_limit: RangeLimit;
  target_summary_tokens: RangeLimit;
  job_interval_seconds: RangeLimit;
  max_scopes_per_tick: RangeLimit;
}

interface TaskExecutionRollupLimits {
  token_limit: RangeLimit;
  round_limit: RangeLimit;
  target_summary_tokens: RangeLimit;
  job_interval_seconds: RangeLimit;
  keep_raw_level0_count: RangeLimit;
  max_level: RangeLimit;
  max_scopes_per_tick: RangeLimit;
}

const DEFAULT_SUMMARY_FORM: SummaryJobConfigForm = {
  enabled: true,
  summary_model_config_id: '',
  token_limit: 6000,
  message_count_limit: 8,
  target_summary_tokens: 700,
  job_interval_seconds: 30,
};

const DEFAULT_SUMMARY_LIMITS: SummaryJobLimits = {
  token_limit: { min: 500 },
  message_count_limit: { min: 1 },
  target_summary_tokens: { min: 200 },
  job_interval_seconds: { min: 10 },
};

const DEFAULT_TASK_EXECUTION_SUMMARY_FORM: TaskExecutionSummaryJobConfigForm = {
  enabled: true,
  summary_model_config_id: '',
  token_limit: 6000,
  round_limit: 8,
  target_summary_tokens: 700,
  job_interval_seconds: 30,
  max_scopes_per_tick: 50,
};

const DEFAULT_TASK_EXECUTION_SUMMARY_LIMITS: TaskExecutionSummaryLimits = {
  token_limit: { min: 500 },
  round_limit: { min: 1 },
  target_summary_tokens: { min: 200 },
  job_interval_seconds: { min: 10 },
  max_scopes_per_tick: { min: 1 },
};

const DEFAULT_TASK_EXECUTION_ROLLUP_FORM: TaskExecutionRollupJobConfigForm = {
  enabled: true,
  summary_model_config_id: '',
  token_limit: 6000,
  round_limit: 50,
  target_summary_tokens: 700,
  job_interval_seconds: 60,
  keep_raw_level0_count: 20,
  max_level: 4,
  max_scopes_per_tick: 50,
};

const DEFAULT_TASK_EXECUTION_ROLLUP_LIMITS: TaskExecutionRollupLimits = {
  token_limit: { min: 500 },
  round_limit: { min: 3 },
  target_summary_tokens: { min: 200 },
  job_interval_seconds: { min: 10 },
  keep_raw_level0_count: { min: 0 },
  max_level: { min: 1 },
  max_scopes_per_tick: { min: 1 },
};

function clampNumber(value: number, range: RangeLimit): number {
  if (!Number.isFinite(value)) {
    return range.min;
  }
  if (Number.isFinite(range.max)) {
    return Math.max(range.min, Math.min(range.max as number, value));
  }
  return Math.max(range.min, value);
}

function rangeText(range: RangeLimit): string {
  if (Number.isFinite(range.max)) {
    return `${range.min}-${range.max}`;
  }
  return `>=${range.min}`;
}

function parseRangeLimit(input: any, fallback: RangeLimit): RangeLimit {
  const min = Number(input?.min);
  const max = Number(input?.max);
  if (Number.isFinite(min) && Number.isFinite(max) && max >= min) {
    return { min, max };
  }
  if (Number.isFinite(min)) {
    return { min };
  }
  return fallback;
}

function parseSummaryLimits(config: any): SummaryJobLimits {
  const limits = config?.limits || {};
  return {
    token_limit: parseRangeLimit(limits?.token_limit, DEFAULT_SUMMARY_LIMITS.token_limit),
    message_count_limit: parseRangeLimit(
      limits?.message_count_limit || limits?.round_limit,
      DEFAULT_SUMMARY_LIMITS.message_count_limit,
    ),
    target_summary_tokens: parseRangeLimit(
      limits?.target_summary_tokens,
      DEFAULT_SUMMARY_LIMITS.target_summary_tokens,
    ),
    job_interval_seconds: parseRangeLimit(
      limits?.job_interval_seconds,
      DEFAULT_SUMMARY_LIMITS.job_interval_seconds,
    ),
  };
}

function parseTaskExecutionSummaryLimits(config: any): TaskExecutionSummaryLimits {
  const limits = config?.limits || {};
  return {
    token_limit: parseRangeLimit(limits?.token_limit, DEFAULT_TASK_EXECUTION_SUMMARY_LIMITS.token_limit),
    round_limit: parseRangeLimit(limits?.round_limit, DEFAULT_TASK_EXECUTION_SUMMARY_LIMITS.round_limit),
    target_summary_tokens: parseRangeLimit(
      limits?.target_summary_tokens,
      DEFAULT_TASK_EXECUTION_SUMMARY_LIMITS.target_summary_tokens,
    ),
    job_interval_seconds: parseRangeLimit(
      limits?.job_interval_seconds,
      DEFAULT_TASK_EXECUTION_SUMMARY_LIMITS.job_interval_seconds,
    ),
    max_scopes_per_tick: parseRangeLimit(
      limits?.max_scopes_per_tick,
      DEFAULT_TASK_EXECUTION_SUMMARY_LIMITS.max_scopes_per_tick,
    ),
  };
}

function parseTaskExecutionRollupLimits(config: any): TaskExecutionRollupLimits {
  const limits = config?.limits || {};
  return {
    token_limit: parseRangeLimit(limits?.token_limit, DEFAULT_TASK_EXECUTION_ROLLUP_LIMITS.token_limit),
    round_limit: parseRangeLimit(limits?.round_limit, DEFAULT_TASK_EXECUTION_ROLLUP_LIMITS.round_limit),
    target_summary_tokens: parseRangeLimit(
      limits?.target_summary_tokens,
      DEFAULT_TASK_EXECUTION_ROLLUP_LIMITS.target_summary_tokens,
    ),
    job_interval_seconds: parseRangeLimit(
      limits?.job_interval_seconds,
      DEFAULT_TASK_EXECUTION_ROLLUP_LIMITS.job_interval_seconds,
    ),
    keep_raw_level0_count: parseRangeLimit(
      limits?.keep_raw_level0_count,
      DEFAULT_TASK_EXECUTION_ROLLUP_LIMITS.keep_raw_level0_count,
    ),
    max_level: parseRangeLimit(limits?.max_level, DEFAULT_TASK_EXECUTION_ROLLUP_LIMITS.max_level),
    max_scopes_per_tick: parseRangeLimit(
      limits?.max_scopes_per_tick,
      DEFAULT_TASK_EXECUTION_ROLLUP_LIMITS.max_scopes_per_tick,
    ),
  };
}

const UserSettingsPanel: React.FC<Props> = ({ onClose }) => {
  const clientFromContext = useChatApiClientFromContext();
  const client = clientFromContext || globalApiClient;
  const { userId } = useChatRuntimeEnv();
  const { aiModelConfigs, loadAiModelConfigs } = useChatStoreFromContext();

  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [notice, setNotice] = React.useState<string | null>(null);
  const [settings, setSettings] = React.useState<any>({});
  const [summaryForm, setSummaryForm] = React.useState<SummaryJobConfigForm>(DEFAULT_SUMMARY_FORM);
  const [summaryLimits, setSummaryLimits] = React.useState<SummaryJobLimits>(DEFAULT_SUMMARY_LIMITS);
  const [taskSummaryForm, setTaskSummaryForm] = React.useState<TaskExecutionSummaryJobConfigForm>(
    DEFAULT_TASK_EXECUTION_SUMMARY_FORM,
  );
  const [taskSummaryLimits, setTaskSummaryLimits] = React.useState<TaskExecutionSummaryLimits>(
    DEFAULT_TASK_EXECUTION_SUMMARY_LIMITS,
  );
  const [taskRollupForm, setTaskRollupForm] = React.useState<TaskExecutionRollupJobConfigForm>(
    DEFAULT_TASK_EXECUTION_ROLLUP_FORM,
  );
  const [taskRollupLimits, setTaskRollupLimits] = React.useState<TaskExecutionRollupLimits>(
    DEFAULT_TASK_EXECUTION_ROLLUP_LIMITS,
  );

  const modelOptions = React.useMemo(
    () =>
      (Array.isArray(aiModelConfigs) ? aiModelConfigs : []).filter(
        (item: any) => item?.enabled === true,
      ),
    [aiModelConfigs],
  );

  React.useEffect(() => {
    if (modelOptions.length === 0) {
      void loadAiModelConfigs();
    }
  }, [loadAiModelConfigs, modelOptions.length]);

  React.useEffect(() => {
    let mounted = true;
    (async () => {
      setLoading(true);
      setError(null);
      try {
        const [settingsResp, summaryResp, taskSummaryResp, taskRollupResp] = await Promise.allSettled([
          client.getUserSettings(userId),
          client.getSessionSummaryJobConfig(userId),
          client.getTaskExecutionSummaryJobConfig(userId),
          client.getTaskExecutionRollupJobConfig(userId),
        ]);
        if (!mounted) return;

        const loadErrors: string[] = [];

        if (settingsResp.status === 'fulfilled') {
          setSettings(settingsResp.value?.effective || {});
        } else {
          loadErrors.push(String(settingsResp.reason?.message || settingsResp.reason || '用户参数加载失败'));
        }

        if (summaryResp.status === 'fulfilled') {
          const loadedLimits = parseSummaryLimits(summaryResp.value);
          setSummaryLimits(loadedLimits);
          setSummaryForm({
            enabled: summaryResp.value?.enabled !== false,
            summary_model_config_id: String(summaryResp.value?.summary_model_config_id || ''),
            token_limit: clampNumber(
              Number(summaryResp.value?.token_limit || DEFAULT_SUMMARY_FORM.token_limit),
              loadedLimits.token_limit,
            ),
            message_count_limit: clampNumber(
              Number(
                summaryResp.value?.message_count_limit
                  || summaryResp.value?.round_limit
                  || DEFAULT_SUMMARY_FORM.message_count_limit,
              ),
              loadedLimits.message_count_limit,
            ),
            target_summary_tokens: clampNumber(
              Number(summaryResp.value?.target_summary_tokens || DEFAULT_SUMMARY_FORM.target_summary_tokens),
              loadedLimits.target_summary_tokens,
            ),
            job_interval_seconds: clampNumber(
              Number(summaryResp.value?.job_interval_seconds || DEFAULT_SUMMARY_FORM.job_interval_seconds),
              loadedLimits.job_interval_seconds,
            ),
          });
        } else {
          loadErrors.push(String(summaryResp.reason?.message || summaryResp.reason || '定时总结配置加载失败'));
        }

        if (taskSummaryResp.status === 'fulfilled') {
          const loadedLimits = parseTaskExecutionSummaryLimits(taskSummaryResp.value);
          setTaskSummaryLimits(loadedLimits);
          setTaskSummaryForm({
            enabled: taskSummaryResp.value?.enabled !== false,
            summary_model_config_id: String(taskSummaryResp.value?.summary_model_config_id || ''),
            token_limit: clampNumber(
              Number(taskSummaryResp.value?.token_limit || DEFAULT_TASK_EXECUTION_SUMMARY_FORM.token_limit),
              loadedLimits.token_limit,
            ),
            round_limit: clampNumber(
              Number(taskSummaryResp.value?.round_limit || DEFAULT_TASK_EXECUTION_SUMMARY_FORM.round_limit),
              loadedLimits.round_limit,
            ),
            target_summary_tokens: clampNumber(
              Number(taskSummaryResp.value?.target_summary_tokens || DEFAULT_TASK_EXECUTION_SUMMARY_FORM.target_summary_tokens),
              loadedLimits.target_summary_tokens,
            ),
            job_interval_seconds: clampNumber(
              Number(taskSummaryResp.value?.job_interval_seconds || DEFAULT_TASK_EXECUTION_SUMMARY_FORM.job_interval_seconds),
              loadedLimits.job_interval_seconds,
            ),
            max_scopes_per_tick: clampNumber(
              Number(taskSummaryResp.value?.max_scopes_per_tick || DEFAULT_TASK_EXECUTION_SUMMARY_FORM.max_scopes_per_tick),
              loadedLimits.max_scopes_per_tick,
            ),
          });
        } else {
          loadErrors.push(String(taskSummaryResp.reason?.message || taskSummaryResp.reason || '任务执行总结配置加载失败'));
        }

        if (taskRollupResp.status === 'fulfilled') {
          const loadedLimits = parseTaskExecutionRollupLimits(taskRollupResp.value);
          setTaskRollupLimits(loadedLimits);
          setTaskRollupForm({
            enabled: taskRollupResp.value?.enabled !== false,
            summary_model_config_id: String(taskRollupResp.value?.summary_model_config_id || ''),
            token_limit: clampNumber(
              Number(taskRollupResp.value?.token_limit || DEFAULT_TASK_EXECUTION_ROLLUP_FORM.token_limit),
              loadedLimits.token_limit,
            ),
            round_limit: clampNumber(
              Number(taskRollupResp.value?.round_limit || DEFAULT_TASK_EXECUTION_ROLLUP_FORM.round_limit),
              loadedLimits.round_limit,
            ),
            target_summary_tokens: clampNumber(
              Number(taskRollupResp.value?.target_summary_tokens || DEFAULT_TASK_EXECUTION_ROLLUP_FORM.target_summary_tokens),
              loadedLimits.target_summary_tokens,
            ),
            job_interval_seconds: clampNumber(
              Number(taskRollupResp.value?.job_interval_seconds || DEFAULT_TASK_EXECUTION_ROLLUP_FORM.job_interval_seconds),
              loadedLimits.job_interval_seconds,
            ),
            keep_raw_level0_count: clampNumber(
              Number(taskRollupResp.value?.keep_raw_level0_count ?? DEFAULT_TASK_EXECUTION_ROLLUP_FORM.keep_raw_level0_count),
              loadedLimits.keep_raw_level0_count,
            ),
            max_level: clampNumber(
              Number(taskRollupResp.value?.max_level || DEFAULT_TASK_EXECUTION_ROLLUP_FORM.max_level),
              loadedLimits.max_level,
            ),
            max_scopes_per_tick: clampNumber(
              Number(taskRollupResp.value?.max_scopes_per_tick || DEFAULT_TASK_EXECUTION_ROLLUP_FORM.max_scopes_per_tick),
              loadedLimits.max_scopes_per_tick,
            ),
          });
        } else {
          loadErrors.push(String(taskRollupResp.reason?.message || taskRollupResp.reason || '任务执行 rollup 配置加载失败'));
        }

        if (loadErrors.length > 0) {
          setError(loadErrors.join('；'));
        }
      } finally {
        if (mounted) setLoading(false);
      }
    })();
    return () => { mounted = false; };
  }, [client, userId]);

  const bind = (key: string) => ({
    value: settings[key] ?? '',
    onChange: (e: React.ChangeEvent<HTMLInputElement>) => {
      const val = e.target.type === 'checkbox' ? e.target.checked : e.target.value;
      setSettings((s: any) => ({ ...s, [key]: e.target.type === 'number' ? Number(val) : val }));
    }
  });

  const setSummaryField = <K extends keyof SummaryJobConfigForm>(key: K, value: SummaryJobConfigForm[K]) => {
    setSummaryForm((prev) => ({ ...prev, [key]: value }));
  };

  const setTaskSummaryField = <K extends keyof TaskExecutionSummaryJobConfigForm>(
    key: K,
    value: TaskExecutionSummaryJobConfigForm[K],
  ) => {
    setTaskSummaryForm((prev) => ({ ...prev, [key]: value }));
  };

  const setTaskRollupField = <K extends keyof TaskExecutionRollupJobConfigForm>(
    key: K,
    value: TaskExecutionRollupJobConfigForm[K],
  ) => {
    setTaskRollupForm((prev) => ({ ...prev, [key]: value }));
  };

  const save = async () => {
    if (!userId) { setError('缺少 userId，无法保存'); return; }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const userSettingsPayload: any = {
        MAX_ITERATIONS: Number(settings.MAX_ITERATIONS || 0),
        LOG_LEVEL: String(settings.LOG_LEVEL || 'info'),
        CHAT_MAX_TOKENS: settings.CHAT_MAX_TOKENS === '' || settings.CHAT_MAX_TOKENS === null || settings.CHAT_MAX_TOKENS === undefined
          ? null
          : Number(settings.CHAT_MAX_TOKENS)
      };

      const rawTokenLimit = Number(summaryForm.token_limit || 0);
      const rawMessageCountLimit = Number(summaryForm.message_count_limit || 0);
      const rawTargetSummaryTokens = Number(summaryForm.target_summary_tokens || 0);
      const rawJobIntervalSeconds = Number(summaryForm.job_interval_seconds || 0);
      const rawTaskSummaryTokenLimit = Number(taskSummaryForm.token_limit || 0);
      const rawTaskSummaryRoundLimit = Number(taskSummaryForm.round_limit || 0);
      const rawTaskSummaryTargetSummaryTokens = Number(taskSummaryForm.target_summary_tokens || 0);
      const rawTaskSummaryJobIntervalSeconds = Number(taskSummaryForm.job_interval_seconds || 0);
      const rawTaskSummaryMaxScopesPerTick = Number(taskSummaryForm.max_scopes_per_tick || 0);
      const rawTaskRollupTokenLimit = Number(taskRollupForm.token_limit || 0);
      const rawTaskRollupRoundLimit = Number(taskRollupForm.round_limit || 0);
      const rawTaskRollupTargetSummaryTokens = Number(taskRollupForm.target_summary_tokens || 0);
      const rawTaskRollupJobIntervalSeconds = Number(taskRollupForm.job_interval_seconds || 0);
      const rawTaskRollupKeepRawLevel0Count = Number(taskRollupForm.keep_raw_level0_count || 0);
      const rawTaskRollupMaxLevel = Number(taskRollupForm.max_level || 0);
      const rawTaskRollupMaxScopesPerTick = Number(taskRollupForm.max_scopes_per_tick || 0);

      const tokenLimit = clampNumber(rawTokenLimit, summaryLimits.token_limit);
      const messageCountLimit = clampNumber(rawMessageCountLimit, summaryLimits.message_count_limit);
      const targetSummaryTokens = clampNumber(rawTargetSummaryTokens, summaryLimits.target_summary_tokens);
      const jobIntervalSeconds = clampNumber(rawJobIntervalSeconds, summaryLimits.job_interval_seconds);
      const taskSummaryTokenLimit = clampNumber(rawTaskSummaryTokenLimit, taskSummaryLimits.token_limit);
      const taskSummaryRoundLimit = clampNumber(rawTaskSummaryRoundLimit, taskSummaryLimits.round_limit);
      const taskSummaryTargetSummaryTokens = clampNumber(
        rawTaskSummaryTargetSummaryTokens,
        taskSummaryLimits.target_summary_tokens,
      );
      const taskSummaryJobIntervalSeconds = clampNumber(
        rawTaskSummaryJobIntervalSeconds,
        taskSummaryLimits.job_interval_seconds,
      );
      const taskSummaryMaxScopesPerTick = clampNumber(
        rawTaskSummaryMaxScopesPerTick,
        taskSummaryLimits.max_scopes_per_tick,
      );
      const taskRollupTokenLimit = clampNumber(rawTaskRollupTokenLimit, taskRollupLimits.token_limit);
      const taskRollupRoundLimit = clampNumber(rawTaskRollupRoundLimit, taskRollupLimits.round_limit);
      const taskRollupTargetSummaryTokens = clampNumber(
        rawTaskRollupTargetSummaryTokens,
        taskRollupLimits.target_summary_tokens,
      );
      const taskRollupJobIntervalSeconds = clampNumber(
        rawTaskRollupJobIntervalSeconds,
        taskRollupLimits.job_interval_seconds,
      );
      const taskRollupKeepRawLevel0Count = clampNumber(
        rawTaskRollupKeepRawLevel0Count,
        taskRollupLimits.keep_raw_level0_count,
      );
      const taskRollupMaxLevel = clampNumber(rawTaskRollupMaxLevel, taskRollupLimits.max_level);
      const taskRollupMaxScopesPerTick = clampNumber(
        rawTaskRollupMaxScopesPerTick,
        taskRollupLimits.max_scopes_per_tick,
      );

      const clampedFields: string[] = [];
      if (tokenLimit !== rawTokenLimit) clampedFields.push(`长度阈值(${rangeText(summaryLimits.token_limit)})`);
      if (messageCountLimit !== rawMessageCountLimit) clampedFields.push(`消息条数阈值(${rangeText(summaryLimits.message_count_limit)})`);
      if (targetSummaryTokens !== rawTargetSummaryTokens) clampedFields.push(`目标摘要长度(${rangeText(summaryLimits.target_summary_tokens)})`);
      if (jobIntervalSeconds !== rawJobIntervalSeconds) clampedFields.push(`任务间隔(${rangeText(summaryLimits.job_interval_seconds)})`);
      if (taskSummaryTokenLimit !== rawTaskSummaryTokenLimit) clampedFields.push(`任务执行总结长度阈值(${rangeText(taskSummaryLimits.token_limit)})`);
      if (taskSummaryRoundLimit !== rawTaskSummaryRoundLimit) clampedFields.push(`任务执行总结轮次阈值(${rangeText(taskSummaryLimits.round_limit)})`);
      if (taskSummaryTargetSummaryTokens !== rawTaskSummaryTargetSummaryTokens) clampedFields.push(`任务执行总结目标长度(${rangeText(taskSummaryLimits.target_summary_tokens)})`);
      if (taskSummaryJobIntervalSeconds !== rawTaskSummaryJobIntervalSeconds) clampedFields.push(`任务执行总结间隔(${rangeText(taskSummaryLimits.job_interval_seconds)})`);
      if (taskSummaryMaxScopesPerTick !== rawTaskSummaryMaxScopesPerTick) clampedFields.push(`任务执行总结每轮 scope 数(${rangeText(taskSummaryLimits.max_scopes_per_tick)})`);
      if (taskRollupTokenLimit !== rawTaskRollupTokenLimit) clampedFields.push(`任务执行 rollup 长度阈值(${rangeText(taskRollupLimits.token_limit)})`);
      if (taskRollupRoundLimit !== rawTaskRollupRoundLimit) clampedFields.push(`任务执行 rollup 轮次阈值(${rangeText(taskRollupLimits.round_limit)})`);
      if (taskRollupTargetSummaryTokens !== rawTaskRollupTargetSummaryTokens) clampedFields.push(`任务执行 rollup 目标长度(${rangeText(taskRollupLimits.target_summary_tokens)})`);
      if (taskRollupJobIntervalSeconds !== rawTaskRollupJobIntervalSeconds) clampedFields.push(`任务执行 rollup 间隔(${rangeText(taskRollupLimits.job_interval_seconds)})`);
      if (taskRollupKeepRawLevel0Count !== rawTaskRollupKeepRawLevel0Count) clampedFields.push(`任务执行 rollup 保留 L0 数(${rangeText(taskRollupLimits.keep_raw_level0_count)})`);
      if (taskRollupMaxLevel !== rawTaskRollupMaxLevel) clampedFields.push(`任务执行 rollup 最大层级(${rangeText(taskRollupLimits.max_level)})`);
      if (taskRollupMaxScopesPerTick !== rawTaskRollupMaxScopesPerTick) clampedFields.push(`任务执行 rollup 每轮 scope 数(${rangeText(taskRollupLimits.max_scopes_per_tick)})`);

      const [savedSettings, savedSummary, savedTaskSummary, savedTaskRollup] = await Promise.all([
        client.updateUserSettings(userId, userSettingsPayload),
        client.updateSessionSummaryJobConfig({
          user_id: userId,
          enabled: summaryForm.enabled,
          summary_model_config_id: summaryForm.summary_model_config_id || null,
          token_limit: tokenLimit,
          message_count_limit: messageCountLimit,
          round_limit: messageCountLimit,
          target_summary_tokens: targetSummaryTokens,
          job_interval_seconds: jobIntervalSeconds,
        }),
        client.updateTaskExecutionSummaryJobConfig({
          user_id: userId,
          enabled: taskSummaryForm.enabled,
          summary_model_config_id: taskSummaryForm.summary_model_config_id || null,
          token_limit: taskSummaryTokenLimit,
          round_limit: taskSummaryRoundLimit,
          target_summary_tokens: taskSummaryTargetSummaryTokens,
          job_interval_seconds: taskSummaryJobIntervalSeconds,
          max_scopes_per_tick: taskSummaryMaxScopesPerTick,
        }),
        client.updateTaskExecutionRollupJobConfig({
          user_id: userId,
          enabled: taskRollupForm.enabled,
          summary_model_config_id: taskRollupForm.summary_model_config_id || null,
          token_limit: taskRollupTokenLimit,
          round_limit: taskRollupRoundLimit,
          target_summary_tokens: taskRollupTargetSummaryTokens,
          job_interval_seconds: taskRollupJobIntervalSeconds,
          keep_raw_level0_count: taskRollupKeepRawLevel0Count,
          max_level: taskRollupMaxLevel,
          max_scopes_per_tick: taskRollupMaxScopesPerTick,
        }),
      ]);

      setSettings(savedSettings?.effective || userSettingsPayload);

      const savedLimits = parseSummaryLimits(savedSummary);
      setSummaryLimits(savedLimits);
      setSummaryForm({
        enabled: savedSummary?.enabled !== false,
        summary_model_config_id: String(savedSummary?.summary_model_config_id || ''),
        token_limit: clampNumber(
          Number(savedSummary?.token_limit || tokenLimit),
          savedLimits.token_limit,
        ),
        message_count_limit: clampNumber(
          Number(savedSummary?.message_count_limit || savedSummary?.round_limit || messageCountLimit),
          savedLimits.message_count_limit,
        ),
        target_summary_tokens: clampNumber(
          Number(savedSummary?.target_summary_tokens || targetSummaryTokens),
          savedLimits.target_summary_tokens,
        ),
        job_interval_seconds: clampNumber(
          Number(savedSummary?.job_interval_seconds || jobIntervalSeconds),
          savedLimits.job_interval_seconds,
        ),
      });

      const savedTaskSummaryLimits = parseTaskExecutionSummaryLimits(savedTaskSummary);
      setTaskSummaryLimits(savedTaskSummaryLimits);
      setTaskSummaryForm({
        enabled: savedTaskSummary?.enabled !== false,
        summary_model_config_id: String(savedTaskSummary?.summary_model_config_id || ''),
        token_limit: clampNumber(
          Number(savedTaskSummary?.token_limit || taskSummaryTokenLimit),
          savedTaskSummaryLimits.token_limit,
        ),
        round_limit: clampNumber(
          Number(savedTaskSummary?.round_limit || taskSummaryRoundLimit),
          savedTaskSummaryLimits.round_limit,
        ),
        target_summary_tokens: clampNumber(
          Number(savedTaskSummary?.target_summary_tokens || taskSummaryTargetSummaryTokens),
          savedTaskSummaryLimits.target_summary_tokens,
        ),
        job_interval_seconds: clampNumber(
          Number(savedTaskSummary?.job_interval_seconds || taskSummaryJobIntervalSeconds),
          savedTaskSummaryLimits.job_interval_seconds,
        ),
        max_scopes_per_tick: clampNumber(
          Number(savedTaskSummary?.max_scopes_per_tick || taskSummaryMaxScopesPerTick),
          savedTaskSummaryLimits.max_scopes_per_tick,
        ),
      });

      const savedTaskRollupLimits = parseTaskExecutionRollupLimits(savedTaskRollup);
      setTaskRollupLimits(savedTaskRollupLimits);
      setTaskRollupForm({
        enabled: savedTaskRollup?.enabled !== false,
        summary_model_config_id: String(savedTaskRollup?.summary_model_config_id || ''),
        token_limit: clampNumber(
          Number(savedTaskRollup?.token_limit || taskRollupTokenLimit),
          savedTaskRollupLimits.token_limit,
        ),
        round_limit: clampNumber(
          Number(savedTaskRollup?.round_limit || taskRollupRoundLimit),
          savedTaskRollupLimits.round_limit,
        ),
        target_summary_tokens: clampNumber(
          Number(savedTaskRollup?.target_summary_tokens || taskRollupTargetSummaryTokens),
          savedTaskRollupLimits.target_summary_tokens,
        ),
        job_interval_seconds: clampNumber(
          Number(savedTaskRollup?.job_interval_seconds || taskRollupJobIntervalSeconds),
          savedTaskRollupLimits.job_interval_seconds,
        ),
        keep_raw_level0_count: clampNumber(
          Number(savedTaskRollup?.keep_raw_level0_count ?? taskRollupKeepRawLevel0Count),
          savedTaskRollupLimits.keep_raw_level0_count,
        ),
        max_level: clampNumber(
          Number(savedTaskRollup?.max_level || taskRollupMaxLevel),
          savedTaskRollupLimits.max_level,
        ),
        max_scopes_per_tick: clampNumber(
          Number(savedTaskRollup?.max_scopes_per_tick || taskRollupMaxScopesPerTick),
          savedTaskRollupLimits.max_scopes_per_tick,
        ),
      });

      if (clampedFields.length > 0) {
        setNotice(`保存成功，定时总结配置已按安全范围自动调整：${clampedFields.join('、')}`);
      } else {
        setNotice('保存成功');
      }
    } catch (e: any) {
      setError(String(e?.message || e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-gradient-to-b from-background/60 to-background/80 backdrop-blur-sm" />
      <div className="relative bg-card text-card-foreground w-full max-w-3xl rounded-xl shadow-2xl border border-border/60">
        <div className="flex items-center justify-between p-4 sm:p-5 border-b border-border/60">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-accent/60 text-accent-foreground">
              <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6V4m0 16v-2m8-6h2M4 12H2m15.364 5.364l1.414 1.414M5.636 6.636L4.222 5.222m12.728 0l1.414 1.414M5.636 17.364l-1.414 1.414" /></svg>
            </div>
            <div>
              <h3 className="font-semibold leading-tight">用户参数设置</h3>
              <p className="text-xs text-muted-foreground mt-0.5">为当前用户定制会话与递归参数</p>
            </div>
          </div>
          <button onClick={onClose} className="p-2 hover:bg-accent rounded-lg transition-colors" aria-label="关闭">
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" /></svg>
          </button>
        </div>
        <div className="p-4 sm:p-6 space-y-4 max-h-[75vh] overflow-auto">
          {loading ? (
            <div className="text-sm text-muted-foreground">加载中...</div>
          ) : (
            <>
              {error && (
                <div className="p-2 text-sm rounded-lg bg-destructive/10 text-destructive border border-destructive/20">{error}</div>
              )}
              {notice && (
                <div className="p-2 text-sm rounded-lg bg-primary/10 text-primary border border-primary/20">{notice}</div>
              )}

              <div className="rounded-xl border border-border/60 overflow-hidden">
                <div className="px-4 py-2.5 border-b border-border/60 bg-accent/10 text-sm font-medium">递归与日志</div>
                <div className="p-4 grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <div>
                    <label className="text-xs text-muted-foreground">最大输出 Tokens（每次回复）</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('CHAT_MAX_TOKENS')} />
                    <p className="text-[11px] text-muted-foreground mt-1">后端只从此处读取。留空则不限制，模型按默认生成。</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">最大递归轮数</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('MAX_ITERATIONS')} />
                    <p className="text-[11px] text-muted-foreground mt-1">一次请求内的工具调用迭代上限，用于防止无限循环。建议: 4-6。</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">日志级别</label>
                    <input type="text" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('LOG_LEVEL')} placeholder="info|warn|error|debug" />
                    <p className="text-[11px] text-muted-foreground mt-1">仅作为本用户偏好保存，不修改服务器全局日志。</p>
                  </div>
                </div>
              </div>

              <div className="rounded-xl border border-border/60 overflow-hidden">
                <div className="px-4 py-2.5 border-b border-border/60 bg-accent/10 text-sm font-medium">定时总结任务</div>
                <div className="p-4 space-y-4">
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="text-sm font-medium">启用定时总结</div>
                      <div className="text-[11px] text-muted-foreground mt-1">关闭后该用户不再生成新的定时总结</div>
                    </div>
                    <input
                      type="checkbox"
                      className="h-4 w-4"
                      checked={summaryForm.enabled}
                      onChange={(event) => setSummaryField('enabled', event.target.checked)}
                    />
                  </div>

                  <div>
                    <label className="text-xs text-muted-foreground">总结模型</label>
                    <select
                      className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                      value={summaryForm.summary_model_config_id}
                      onChange={(event) => setSummaryField('summary_model_config_id', event.target.value)}
                    >
                      <option value="">默认模型（环境变量）</option>
                      {modelOptions.map((option: any) => (
                        <option key={option.id} value={option.id}>
                          {option.name}（{option.model_name || 'unknown'}）
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div>
                      <label className="text-xs text-muted-foreground">长度阈值（Token，{rangeText(summaryLimits.token_limit)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={summaryForm.token_limit}
                        min={summaryLimits.token_limit.min}
                        max={summaryLimits.token_limit.max}
                        onChange={(event) => setSummaryField('token_limit', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">消息条数阈值（{rangeText(summaryLimits.message_count_limit)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={summaryForm.message_count_limit}
                        min={summaryLimits.message_count_limit.min}
                        max={summaryLimits.message_count_limit.max}
                        onChange={(event) => setSummaryField('message_count_limit', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">目标摘要长度（Token，{rangeText(summaryLimits.target_summary_tokens)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={summaryForm.target_summary_tokens}
                        min={summaryLimits.target_summary_tokens.min}
                        max={summaryLimits.target_summary_tokens.max}
                        onChange={(event) => setSummaryField('target_summary_tokens', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">任务间隔（秒，{rangeText(summaryLimits.job_interval_seconds)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={summaryForm.job_interval_seconds}
                        min={summaryLimits.job_interval_seconds.min}
                        max={summaryLimits.job_interval_seconds.max}
                        onChange={(event) => setSummaryField('job_interval_seconds', Number(event.target.value || 0))}
                      />
                    </div>
                  </div>
                </div>
              </div>

              <div className="rounded-xl border border-border/60 overflow-hidden">
                <div className="px-4 py-2.5 border-b border-border/60 bg-accent/10 text-sm font-medium">任务执行总结</div>
                <div className="p-4 space-y-4">
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="text-sm font-medium">启用任务执行总结</div>
                      <div className="text-[11px] text-muted-foreground mt-1">用于压缩后台任务执行过程，保障后续任务衔接</div>
                    </div>
                    <input
                      type="checkbox"
                      className="h-4 w-4"
                      checked={taskSummaryForm.enabled}
                      onChange={(event) => setTaskSummaryField('enabled', event.target.checked)}
                    />
                  </div>

                  <div>
                    <label className="text-xs text-muted-foreground">总结模型</label>
                    <select
                      className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                      value={taskSummaryForm.summary_model_config_id}
                      onChange={(event) => setTaskSummaryField('summary_model_config_id', event.target.value)}
                    >
                      <option value="">默认模型（环境变量）</option>
                      {modelOptions.map((option: any) => (
                        <option key={option.id} value={option.id}>
                          {option.name}（{option.model_name || 'unknown'}）
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div>
                      <label className="text-xs text-muted-foreground">长度阈值（Token，{rangeText(taskSummaryLimits.token_limit)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskSummaryForm.token_limit}
                        min={taskSummaryLimits.token_limit.min}
                        max={taskSummaryLimits.token_limit.max}
                        onChange={(event) => setTaskSummaryField('token_limit', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">执行轮次阈值（{rangeText(taskSummaryLimits.round_limit)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskSummaryForm.round_limit}
                        min={taskSummaryLimits.round_limit.min}
                        max={taskSummaryLimits.round_limit.max}
                        onChange={(event) => setTaskSummaryField('round_limit', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">目标摘要长度（Token，{rangeText(taskSummaryLimits.target_summary_tokens)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskSummaryForm.target_summary_tokens}
                        min={taskSummaryLimits.target_summary_tokens.min}
                        max={taskSummaryLimits.target_summary_tokens.max}
                        onChange={(event) => setTaskSummaryField('target_summary_tokens', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">任务间隔（秒，{rangeText(taskSummaryLimits.job_interval_seconds)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskSummaryForm.job_interval_seconds}
                        min={taskSummaryLimits.job_interval_seconds.min}
                        max={taskSummaryLimits.job_interval_seconds.max}
                        onChange={(event) => setTaskSummaryField('job_interval_seconds', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">每轮最多扫描 scope 数（{rangeText(taskSummaryLimits.max_scopes_per_tick)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskSummaryForm.max_scopes_per_tick}
                        min={taskSummaryLimits.max_scopes_per_tick.min}
                        max={taskSummaryLimits.max_scopes_per_tick.max}
                        onChange={(event) => setTaskSummaryField('max_scopes_per_tick', Number(event.target.value || 0))}
                      />
                    </div>
                  </div>
                </div>
              </div>

              <div className="rounded-xl border border-border/60 overflow-hidden">
                <div className="px-4 py-2.5 border-b border-border/60 bg-accent/10 text-sm font-medium">任务执行 Rollup</div>
                <div className="p-4 space-y-4">
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="text-sm font-medium">启用任务执行 Rollup</div>
                      <div className="text-[11px] text-muted-foreground mt-1">把多段任务执行总结继续压缩成高层总结，供长期衔接与回忆使用</div>
                    </div>
                    <input
                      type="checkbox"
                      className="h-4 w-4"
                      checked={taskRollupForm.enabled}
                      onChange={(event) => setTaskRollupField('enabled', event.target.checked)}
                    />
                  </div>

                  <div>
                    <label className="text-xs text-muted-foreground">Rollup 模型</label>
                    <select
                      className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                      value={taskRollupForm.summary_model_config_id}
                      onChange={(event) => setTaskRollupField('summary_model_config_id', event.target.value)}
                    >
                      <option value="">默认模型（环境变量）</option>
                      {modelOptions.map((option: any) => (
                        <option key={option.id} value={option.id}>
                          {option.name}（{option.model_name || 'unknown'}）
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div>
                      <label className="text-xs text-muted-foreground">长度阈值（Token，{rangeText(taskRollupLimits.token_limit)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskRollupForm.token_limit}
                        min={taskRollupLimits.token_limit.min}
                        max={taskRollupLimits.token_limit.max}
                        onChange={(event) => setTaskRollupField('token_limit', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">参与聚合的 L0 条数阈值（{rangeText(taskRollupLimits.round_limit)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskRollupForm.round_limit}
                        min={taskRollupLimits.round_limit.min}
                        max={taskRollupLimits.round_limit.max}
                        onChange={(event) => setTaskRollupField('round_limit', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">目标摘要长度（Token，{rangeText(taskRollupLimits.target_summary_tokens)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskRollupForm.target_summary_tokens}
                        min={taskRollupLimits.target_summary_tokens.min}
                        max={taskRollupLimits.target_summary_tokens.max}
                        onChange={(event) => setTaskRollupField('target_summary_tokens', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">任务间隔（秒，{rangeText(taskRollupLimits.job_interval_seconds)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskRollupForm.job_interval_seconds}
                        min={taskRollupLimits.job_interval_seconds.min}
                        max={taskRollupLimits.job_interval_seconds.max}
                        onChange={(event) => setTaskRollupField('job_interval_seconds', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">保留未聚合的 L0 数量（{rangeText(taskRollupLimits.keep_raw_level0_count)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskRollupForm.keep_raw_level0_count}
                        min={taskRollupLimits.keep_raw_level0_count.min}
                        max={taskRollupLimits.keep_raw_level0_count.max}
                        onChange={(event) => setTaskRollupField('keep_raw_level0_count', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">最大 Rollup 层级（{rangeText(taskRollupLimits.max_level)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskRollupForm.max_level}
                        min={taskRollupLimits.max_level.min}
                        max={taskRollupLimits.max_level.max}
                        onChange={(event) => setTaskRollupField('max_level', Number(event.target.value || 0))}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground">每轮最多扫描 scope 数（{rangeText(taskRollupLimits.max_scopes_per_tick)}）</label>
                      <input
                        type="number"
                        className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                        value={taskRollupForm.max_scopes_per_tick}
                        min={taskRollupLimits.max_scopes_per_tick.min}
                        max={taskRollupLimits.max_scopes_per_tick.max}
                        onChange={(event) => setTaskRollupField('max_scopes_per_tick', Number(event.target.value || 0))}
                      />
                    </div>
                  </div>
                </div>
              </div>
            </>
          )}
        </div>
        <div className="p-4 sm:p-5 border-t border-border/60 flex items-center justify-end gap-2">
          <button onClick={onClose} className="px-3 py-2 rounded-lg bg-muted text-foreground hover:bg-muted/80">取消</button>
          <button onClick={save} disabled={saving} className="px-3 py-2 rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50">{saving ? '保存中...' : '保存'}</button>
        </div>
      </div>
    </div>
  );
};

export default UserSettingsPanel;
