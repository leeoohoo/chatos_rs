import React from "react";
import {
  useChatApiClientFromContext,
  useChatRuntimeEnv,
  useChatStoreFromContext,
} from "../lib/store/ChatStoreContext";
import { apiClient as globalApiClient } from "../lib/api/client";
import {
  buildSummaryForm,
  clampNumber,
  DEFAULT_SUMMARY_FORM,
  DEFAULT_SUMMARY_LIMITS,
  getErrorMessage,
  parseSummaryLimits,
  rangeText,
  type SummaryJobConfigForm,
  type SummaryJobLimits,
} from "./settings/summaryJobConfig";

interface Props {
  onClose: () => void;
}

const SessionSummaryJobConfigPanel: React.FC<Props> = ({ onClose }) => {
  const clientFromContext = useChatApiClientFromContext();
  const client = clientFromContext || globalApiClient;
  const { userId } = useChatRuntimeEnv();
  const { aiModelConfigs, loadAiModelConfigs } = useChatStoreFromContext();
  const effectiveUserId = userId;

  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [notice, setNotice] = React.useState<string | null>(null);
  const [form, setForm] = React.useState<SummaryJobConfigForm>(DEFAULT_SUMMARY_FORM);
  const [limits, setLimits] = React.useState<SummaryJobLimits>(DEFAULT_SUMMARY_LIMITS);

  const modelOptions = React.useMemo(
    () =>
      (Array.isArray(aiModelConfigs) ? aiModelConfigs : []).filter(
        (item) => item.enabled === true
      ),
    [aiModelConfigs]
  );

  React.useEffect(() => {
    if (modelOptions.length === 0) {
      void loadAiModelConfigs();
    }
  }, [loadAiModelConfigs, modelOptions.length]);

  React.useEffect(() => {
    let mounted = true;

    (async () => {
      try {
        setLoading(true);
        const config = await client.getConversationSummaryJobConfig(effectiveUserId);
        const loadedLimits = parseSummaryLimits(config);

        if (!mounted) {
          return;
        }

        setLimits(loadedLimits);
        setForm(buildSummaryForm(config, loadedLimits));
      } catch (e: unknown) {
        if (mounted) {
          setError(getErrorMessage(e));
        }
      } finally {
        if (mounted) {
          setLoading(false);
        }
      }
    })();

    return () => {
      mounted = false;
    };
  }, [client, effectiveUserId]);

  const setField = <K extends keyof SummaryJobConfigForm>(
    key: K,
    value: SummaryJobConfigForm[K]
  ) => {
    setForm((prev) => ({ ...prev, [key]: value }));
  };

  const onSave = async () => {
    setSaving(true);
    setError(null);
    setNotice(null);

    try {
      const rawTokenLimit = Number(form.token_limit || 0);
      const rawMessageCountLimit = Number(form.message_count_limit || 0);
      const rawTargetSummaryTokens = Number(form.target_summary_tokens || 0);
      const rawJobIntervalSeconds = Number(form.job_interval_seconds || 0);

      const tokenLimit = clampNumber(rawTokenLimit, limits.token_limit);
      const messageCountLimit = clampNumber(rawMessageCountLimit, limits.message_count_limit);
      const targetSummaryTokens = clampNumber(
        rawTargetSummaryTokens,
        limits.target_summary_tokens
      );
      const jobIntervalSeconds = clampNumber(rawJobIntervalSeconds, limits.job_interval_seconds);

      const clampedFields: string[] = [];
      if (tokenLimit !== rawTokenLimit) {
        clampedFields.push(`长度阈值(${rangeText(limits.token_limit)})`);
      }
      if (messageCountLimit !== rawMessageCountLimit) {
        clampedFields.push(`消息条数阈值(${rangeText(limits.message_count_limit)})`);
      }
      if (targetSummaryTokens !== rawTargetSummaryTokens) {
        clampedFields.push(`目标摘要长度(${rangeText(limits.target_summary_tokens)})`);
      }
      if (jobIntervalSeconds !== rawJobIntervalSeconds) {
        clampedFields.push(`任务间隔(${rangeText(limits.job_interval_seconds)})`);
      }

      const saved = await client.updateConversationSummaryJobConfig({
        user_id: effectiveUserId,
        enabled: form.enabled,
        summary_model_config_id: form.summary_model_config_id || null,
        token_limit: tokenLimit,
        message_count_limit: messageCountLimit,
        round_limit: messageCountLimit,
        target_summary_tokens: targetSummaryTokens,
        job_interval_seconds: jobIntervalSeconds,
      });

      const savedLimits = parseSummaryLimits(saved);
      setLimits(savedLimits);
      setForm(buildSummaryForm(saved, savedLimits, {
        enabled: form.enabled,
        summary_model_config_id: form.summary_model_config_id,
        token_limit: tokenLimit,
        message_count_limit: messageCountLimit,
        target_summary_tokens: targetSummaryTokens,
        job_interval_seconds: jobIntervalSeconds,
      }));

      if (clampedFields.length > 0) {
        setNotice(`已按安全范围自动调整：${clampedFields.join("、")}`);
      } else {
        setNotice("保存成功");
      }
    } catch (e: unknown) {
      setError(getErrorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-gradient-to-b from-background/60 to-background/80 backdrop-blur-sm" />
      <div className="relative bg-card text-card-foreground w-full max-w-2xl rounded-xl shadow-2xl border border-border/60">
        <div className="flex items-center justify-between p-4 sm:p-5 border-b border-border/60">
          <div>
            <h3 className="font-semibold leading-tight">会话总结任务配置</h3>
            <p className="text-xs text-muted-foreground mt-0.5">
              配置定时总结模型、长度阈值与消息条数阈值
            </p>
          </div>
          <button onClick={onClose} className="p-2 hover:bg-accent rounded-lg transition-colors" aria-label="关闭">
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="p-4 sm:p-6 space-y-4 max-h-[70vh] overflow-auto">
          {loading ? <div className="text-sm text-muted-foreground">加载中...</div> : null}

          {error ? (
            <div className="p-2 text-sm rounded-lg bg-destructive/10 text-destructive border border-destructive/20">
              {error}
            </div>
          ) : null}

          {notice ? (
            <div className="p-2 text-sm rounded-lg bg-primary/10 text-primary border border-primary/20">
              {notice}
            </div>
          ) : null}

          {!loading ? (
            <>
              <div className="rounded-xl border border-border/60 p-4 space-y-4">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-sm font-medium">启用定时总结任务</div>
                    <div className="text-xs text-muted-foreground mt-1">关闭后后台任务不会为该用户生成新总结</div>
                  </div>
                  <input
                    type="checkbox"
                    className="h-4 w-4"
                    checked={form.enabled}
                    onChange={(event) => setField("enabled", event.target.checked)}
                  />
                </div>

                <div>
                  <label className="text-xs text-muted-foreground">总结模型</label>
                  <select
                    className="w-full mt-1 p-2 border rounded-lg bg-background"
                    value={form.summary_model_config_id}
                    onChange={(event) => setField("summary_model_config_id", event.target.value)}
                  >
                    <option value="">默认模型（环境变量）</option>
                    {modelOptions.map((option) => (
                      <option key={option.id} value={option.id}>
                        {option.name} ({option.model_name || "unknown"})
                      </option>
                    ))}
                  </select>
                </div>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 rounded-xl border border-border/60 p-4">
                <div>
                  <label className="text-xs text-muted-foreground">
                    长度阈值（Token，{rangeText(limits.token_limit)}）
                  </label>
                  <input
                    type="number"
                    className="w-full mt-1 p-2 border rounded-lg bg-background"
                    value={form.token_limit}
                    min={limits.token_limit.min}
                    max={limits.token_limit.max}
                    onChange={(event) => setField("token_limit", Number(event.target.value || 0))}
                  />
                </div>
                <div>
                  <label className="text-xs text-muted-foreground">
                    消息条数阈值（{rangeText(limits.message_count_limit)}）
                  </label>
                  <input
                    type="number"
                    className="w-full mt-1 p-2 border rounded-lg bg-background"
                    value={form.message_count_limit}
                    min={limits.message_count_limit.min}
                    max={limits.message_count_limit.max}
                    onChange={(event) =>
                      setField("message_count_limit", Number(event.target.value || 0))
                    }
                  />
                </div>
                <div>
                  <label className="text-xs text-muted-foreground">
                    目标摘要长度（Token，{rangeText(limits.target_summary_tokens)}）
                  </label>
                  <input
                    type="number"
                    className="w-full mt-1 p-2 border rounded-lg bg-background"
                    value={form.target_summary_tokens}
                    min={limits.target_summary_tokens.min}
                    max={limits.target_summary_tokens.max}
                    onChange={(event) =>
                      setField("target_summary_tokens", Number(event.target.value || 0))
                    }
                  />
                </div>
                <div>
                  <label className="text-xs text-muted-foreground">
                    任务间隔（秒，{rangeText(limits.job_interval_seconds)}）
                  </label>
                  <input
                    type="number"
                    className="w-full mt-1 p-2 border rounded-lg bg-background"
                    value={form.job_interval_seconds}
                    min={limits.job_interval_seconds.min}
                    max={limits.job_interval_seconds.max}
                    onChange={(event) =>
                      setField("job_interval_seconds", Number(event.target.value || 0))
                    }
                  />
                </div>
              </div>
            </>
          ) : null}
        </div>

        <div className="p-4 sm:p-5 border-t border-border/60 flex items-center justify-end gap-2">
          <button onClick={onClose} className="px-3 py-2 rounded-lg bg-muted text-foreground hover:bg-muted/80">
            关闭
          </button>
          <button
            onClick={onSave}
            disabled={loading || saving}
            className="px-3 py-2 rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            {saving ? "保存中..." : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
};

export default SessionSummaryJobConfigPanel;
