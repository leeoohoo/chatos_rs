import React from 'react';
import {
  useChatApiClientFromContext,
  useChatRuntimeEnv,
} from '../lib/store/ChatStoreContext';
import { apiClient as globalApiClient } from '../lib/api/client';

interface Props { onClose: () => void }

interface UserSettingsForm {
  MAX_ITERATIONS?: number | string;
  LOG_LEVEL?: string;
  CHAT_MAX_TOKENS?: number | string | null;
  [key: string]: string | number | boolean | null | undefined;
}

interface UserSettingsPayload {
  MAX_ITERATIONS: number;
  LOG_LEVEL: string;
  CHAT_MAX_TOKENS: number | null;
  [key: string]: string | number | null;
}

const normalizeUserSettingsForm = (value: unknown): UserSettingsForm => {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return {};
  }

  const result: UserSettingsForm = {};
  Object.entries(value as Record<string, unknown>).forEach(([key, entry]) => {
    if (
      typeof entry === 'string'
      || typeof entry === 'number'
      || typeof entry === 'boolean'
      || entry === null
      || entry === undefined
    ) {
      result[key] = entry;
    }
  });
  return result;
};

const getErrorMessage = (error: unknown): string => {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  return '操作失败';
};

const UserSettingsPanel: React.FC<Props> = ({ onClose }) => {
  const clientFromContext = useChatApiClientFromContext();
  const client = clientFromContext || globalApiClient;
  const { userId } = useChatRuntimeEnv();

  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [notice, setNotice] = React.useState<string | null>(null);
  const [settings, setSettings] = React.useState<UserSettingsForm>({});

  React.useEffect(() => {
    let mounted = true;
    (async () => {
      setLoading(true);
      setError(null);
      try {
        const settingsResp = await client.getUserSettings(userId);
        if (!mounted) return;
        setSettings(normalizeUserSettingsForm(settingsResp?.effective));
      } catch (e: unknown) {
        if (mounted) {
          setError(getErrorMessage(e));
        }
      } finally {
        if (mounted) setLoading(false);
      }
    })();
    return () => { mounted = false; };
  }, [client, userId]);

  const bind = (key: string) => ({
    value: typeof settings[key] === 'string' || typeof settings[key] === 'number'
      ? settings[key]
      : '',
    onChange: (e: React.ChangeEvent<HTMLInputElement>) => {
      const val = e.target.type === 'checkbox' ? e.target.checked : e.target.value;
      setSettings((s) => ({ ...s, [key]: e.target.type === 'number' ? Number(val) : val }));
    }
  });

  const save = async () => {
    if (!userId) { setError('缺少 userId，无法保存'); return; }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const userSettingsPayload: UserSettingsPayload = {
        MAX_ITERATIONS: Number(settings.MAX_ITERATIONS || 0),
        LOG_LEVEL: String(settings.LOG_LEVEL || 'info'),
        CHAT_MAX_TOKENS: settings.CHAT_MAX_TOKENS === '' || settings.CHAT_MAX_TOKENS === null || settings.CHAT_MAX_TOKENS === undefined
          ? null
          : Number(settings.CHAT_MAX_TOKENS)
      };

      const savedSettings = await client.updateUserSettings(userId, userSettingsPayload);

      setSettings(normalizeUserSettingsForm(savedSettings?.effective || userSettingsPayload));
      setNotice('保存成功');
    } catch (e: unknown) {
      setError(getErrorMessage(e));
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
              <h3 className="font-semibold leading-tight">运行参数</h3>
              <p className="text-xs text-muted-foreground mt-0.5">管理当前账号在 chatos 内的运行参数</p>
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
