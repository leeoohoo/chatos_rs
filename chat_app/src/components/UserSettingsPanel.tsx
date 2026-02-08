import React from 'react';
import { useChatApiClientFromContext, useChatRuntimeEnv } from '../lib/store/ChatStoreContext';
import { apiClient as globalApiClient } from '../lib/api/client';

interface Props { onClose: () => void }

const UserSettingsPanel: React.FC<Props> = ({ onClose }) => {
  const clientFromContext = useChatApiClientFromContext();
  const client = clientFromContext || globalApiClient;
  const { userId } = useChatRuntimeEnv();

  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [settings, setSettings] = React.useState<any>({});

  React.useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const data = await client.getUserSettings(userId);
        if (!mounted) return;
        setSettings(data?.effective || {});
      } catch (e: any) {
        setError(String(e?.message || e));
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

  const save = async () => {
    if (!userId) { setError('缺少 userId，无法保存'); return; }
    setSaving(true);
    setError(null);
    try {
      const payload: any = {
        SUMMARY_ENABLED: Boolean(settings.SUMMARY_ENABLED),
        DYNAMIC_SUMMARY_ENABLED: Boolean(settings.DYNAMIC_SUMMARY_ENABLED),
        SUMMARY_MESSAGE_LIMIT: Number(settings.SUMMARY_MESSAGE_LIMIT || 0),
        SUMMARY_MAX_CONTEXT_TOKENS: Number(settings.SUMMARY_MAX_CONTEXT_TOKENS || 0),
        SUMMARY_KEEP_LAST_N: Number(settings.SUMMARY_KEEP_LAST_N || 0),
        SUMMARY_TARGET_TOKENS: Number(settings.SUMMARY_TARGET_TOKENS || 0),
        SUMMARY_COOLDOWN_SECONDS: Number(settings.SUMMARY_COOLDOWN_SECONDS || 0),
        MAX_ITERATIONS: Number(settings.MAX_ITERATIONS || 0),
        HISTORY_LIMIT: Number(settings.HISTORY_LIMIT || 0),
        LOG_LEVEL: String(settings.LOG_LEVEL || 'info'),
        // New: per-user chat max tokens used by backend only
        CHAT_MAX_TOKENS: settings.CHAT_MAX_TOKENS === '' || settings.CHAT_MAX_TOKENS === null || settings.CHAT_MAX_TOKENS === undefined
          ? null
          : Number(settings.CHAT_MAX_TOKENS)
      };
      const resp = await client.updateUserSettings(userId, payload);
      setSettings(resp?.effective || payload);
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
              <p className="text-xs text-muted-foreground mt-0.5">为当前用户定制会话摘要与递归参数</p>
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
              {/* 开关区块 */}
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div className="p-3 rounded-lg border border-border/60 bg-muted/40">
                  <div className="flex items-center justify-between">
                    <div className="text-sm font-medium">启用会话摘要</div>
                    <label className="inline-flex items-center gap-2">
                      <input type="checkbox" className="h-4 w-4" checked={!!settings.SUMMARY_ENABLED} onChange={(e) => setSettings((s: any) => ({ ...s, SUMMARY_ENABLED: e.target.checked }))} />
                    </label>
                  </div>
                  <div className="text-xs text-muted-foreground mt-1 leading-relaxed">
                    当对话较长时，将早期内容压缩为摘要，作为系统提示继续对话。
                    优点: 降低上下文长度、减少费用；风险: 可能丢失细节。建议保持开启。
                  </div>
                </div>
                <div className="p-3 rounded-lg border border-border/60 bg-muted/40">
                  <div className="flex items-center justify-between">
                    <div className="text-sm font-medium">启用动态摘要</div>
                    <label className="inline-flex items-center gap-2">
                      <input type="checkbox" className="h-4 w-4" checked={!!settings.DYNAMIC_SUMMARY_ENABLED} onChange={(e) => setSettings((s: any) => ({ ...s, DYNAMIC_SUMMARY_ENABLED: e.target.checked }))} />
                    </label>
                  </div>
                  <div className="text-xs text-muted-foreground mt-1 leading-relaxed">
                    在工具调用/多轮递归过程中按需实时压缩历史。更稳定，但会有少量额外延迟与开销。建议开启。
                  </div>
                </div>
              </div>

              {/* 摘要参数 */}
              <div className="rounded-xl border border-border/60 overflow-hidden">
                <div className="px-4 py-2.5 border-b border-border/60 bg-accent/10 text-sm font-medium">摘要参数</div>
                <div className="p-4 grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <div>
                    <label className="text-xs text-muted-foreground">消息阈值</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('SUMMARY_MESSAGE_LIMIT')} />
                    <p className="text-[11px] text-muted-foreground mt-1">达到该消息数量即触发摘要（与 Token 阈值择一触发更早者）。建议: 50–150。</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">最大上下文 Tokens</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('SUMMARY_MAX_CONTEXT_TOKENS')} />
                    <p className="text-[11px] text-muted-foreground mt-1">估算上下文超过该 Token 值时触发摘要。建议: 3000–8000；小模型或移动端可更低。</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">保留最近 N 条</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('SUMMARY_KEEP_LAST_N')} />
                    <p className="text-[11px] text-muted-foreground mt-1">摘要后保留最近 N 条原文，确保近期上下文完整。建议: 3–8。</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">目标摘要 Tokens</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('SUMMARY_TARGET_TOKENS')} />
                    <p className="text-[11px] text-muted-foreground mt-1">摘要目标长度。越大信息保留越多、费用更高。建议: 300–1000（常用 500）。</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">摘要冷却 (秒)</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('SUMMARY_COOLDOWN_SECONDS')} />
                    <p className="text-[11px] text-muted-foreground mt-1">两次摘要之间的最小间隔，避免频繁触发。建议: 30–120 秒。</p>
                  </div>
                </div>
              </div>

              {/* 递归与日志 */}
              <div className="rounded-xl border border-border/60 overflow-hidden">
                <div className="px-4 py-2.5 border-b border-border/60 bg-accent/10 text-sm font-medium">递归与日志</div>
                <div className="p-4 grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <div>
                    <label className="text-xs text-muted-foreground">最大输出 Tokens（每次回复）</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('CHAT_MAX_TOKENS')} />
                    <p className="text-[11px] text-muted-foreground mt-1">后端只从此处读取。留空则不限制，模型按默认生成。</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">历史消息条数</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('HISTORY_LIMIT')} />
                    <p className="text-[11px] text-muted-foreground mt-1">每次请求从历史中取最近 N 条消息加入上下文。设为 0 则不带历史。建议: 10–50。</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">最大递归轮数</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('MAX_ITERATIONS')} />
                    <p className="text-[11px] text-muted-foreground mt-1">一次请求内的工具调用迭代上限，用于防止无限循环。建议: 4–6（调试可临时提高）。</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">日志级别</label>
                    <input type="text" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('LOG_LEVEL')} placeholder="info|warn|error|debug" />
                    <p className="text-[11px] text-muted-foreground mt-1">仅作为本用户偏好保存，不修改服务器全局日志。debug 最详细，error 最少。</p>
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
