import React from 'react';
import {
  useChatApiClientFromContext,
  useChatRuntimeEnv,
} from '../lib/store/ChatStoreContext';
import { apiClient as globalApiClient } from '../lib/api/client';
import { useI18n } from '../i18n/I18nProvider';

interface Props { onClose: () => void }

interface UserSettingsForm {
  MAX_ITERATIONS?: number | string;
  LOG_LEVEL?: string;
  CHAT_MAX_TOKENS?: number | string | null;
  INTERNAL_CONTEXT_LOCALE?: string;
  UI_LOCALE?: string;
  [key: string]: string | number | boolean | null | undefined;
}

interface UserSettingsPayload {
  MAX_ITERATIONS: number;
  LOG_LEVEL: string;
  CHAT_MAX_TOKENS: number | null;
  INTERNAL_CONTEXT_LOCALE: string;
  UI_LOCALE: string;
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

const UserSettingsPanel: React.FC<Props> = ({ onClose }) => {
  const clientFromContext = useChatApiClientFromContext();
  const client = clientFromContext || globalApiClient;
  const { userId } = useChatRuntimeEnv();
  const { locale, setLocale, t } = useI18n();

  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [notice, setNotice] = React.useState<string | null>(null);
  const [settings, setSettings] = React.useState<UserSettingsForm>({});

  const getErrorMessage = React.useCallback((err: unknown): string => {
    if (err instanceof Error) {
      return err.message;
    }
    if (typeof err === 'string') {
      return err;
    }
    return t('common.unknown');
  }, [t]);

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
    if (!userId) { setError(t('settings.missingUserId')); return; }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const nextUiLocale = String(settings.UI_LOCALE || locale || 'zh-CN') === 'en-US' ? 'en-US' : 'zh-CN';
      const userSettingsPayload: UserSettingsPayload = {
        MAX_ITERATIONS: Number(settings.MAX_ITERATIONS || 0),
        LOG_LEVEL: String(settings.LOG_LEVEL || 'info'),
        CHAT_MAX_TOKENS: settings.CHAT_MAX_TOKENS === '' || settings.CHAT_MAX_TOKENS === null || settings.CHAT_MAX_TOKENS === undefined
          ? null
          : Number(settings.CHAT_MAX_TOKENS),
        INTERNAL_CONTEXT_LOCALE: String(settings.INTERNAL_CONTEXT_LOCALE || 'zh-CN'),
        UI_LOCALE: nextUiLocale,
      };

      const savedSettings = await client.updateUserSettings(userId, userSettingsPayload);

      setSettings(normalizeUserSettingsForm(savedSettings?.effective || userSettingsPayload));
      setLocale(nextUiLocale);
      setNotice(t('settings.saved'));
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
              <h3 className="font-semibold leading-tight">{t('settings.title')}</h3>
              <p className="text-xs text-muted-foreground mt-0.5">{t('settings.subtitle')}</p>
              </div>
            </div>
          <button onClick={onClose} className="p-2 hover:bg-accent rounded-lg transition-colors" aria-label={t('common.close')}>
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" /></svg>
          </button>
        </div>
        <div className="p-4 sm:p-6 space-y-4 max-h-[75vh] overflow-auto">
          {loading ? (
            <div className="text-sm text-muted-foreground">{t('common.loading')}</div>
          ) : (
            <>
              {error && (
                <div className="p-2 text-sm rounded-lg bg-destructive/10 text-destructive border border-destructive/20">{error}</div>
              )}
              {notice && (
                <div className="p-2 text-sm rounded-lg bg-primary/10 text-primary border border-primary/20">{notice}</div>
              )}

              <div className="rounded-xl border border-border/60 overflow-hidden">
                <div className="px-4 py-2.5 border-b border-border/60 bg-accent/10 text-sm font-medium">{t('settings.section.runtime')}</div>
                <div className="p-4 grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <div>
                    <label className="text-xs text-muted-foreground">{t('settings.chatMaxTokens')}</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('CHAT_MAX_TOKENS')} />
                    <p className="text-[11px] text-muted-foreground mt-1">{t('settings.chatMaxTokensHelp')}</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">{t('settings.maxIterations')}</label>
                    <input type="number" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('MAX_ITERATIONS')} />
                    <p className="text-[11px] text-muted-foreground mt-1">{t('settings.maxIterationsHelp')}</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">{t('settings.logLevel')}</label>
                    <input type="text" className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40" {...bind('LOG_LEVEL')} placeholder={t('settings.logLevelPlaceholder')} />
                    <p className="text-[11px] text-muted-foreground mt-1">{t('settings.logLevelHelp')}</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">{t('settings.uiLocale')}</label>
                    <select
                      className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                      value={typeof settings.UI_LOCALE === 'string' ? settings.UI_LOCALE : locale}
                      onChange={(e) => {
                        const next = e.target.value === 'en-US' ? 'en-US' : 'zh-CN';
                        setSettings((s) => ({ ...s, UI_LOCALE: next }));
                        setLocale(next);
                      }}
                    >
                      <option value="zh-CN">{t('language.chinese')}</option>
                      <option value="en-US">{t('language.english')}</option>
                    </select>
                    <p className="text-[11px] text-muted-foreground mt-1">{t('settings.uiLocaleHelp')}</p>
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground">{t('settings.internalContextLocale')}</label>
                    <select
                      className="w-full mt-1 p-2 border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary/40"
                      value={typeof settings.INTERNAL_CONTEXT_LOCALE === 'string' ? settings.INTERNAL_CONTEXT_LOCALE : 'zh-CN'}
                      onChange={(e) => {
                        const next = e.target.value === 'en-US' ? 'en-US' : 'zh-CN';
                        setSettings((s) => ({ ...s, INTERNAL_CONTEXT_LOCALE: next }));
                      }}
                    >
                      <option value="zh-CN">{t('language.chinese')}</option>
                      <option value="en-US">{t('language.english')}</option>
                    </select>
                    <p className="text-[11px] text-muted-foreground mt-1">{t('settings.internalContextLocaleHelp')}</p>
                  </div>
                </div>
              </div>
            </>
          )}
        </div>
        <div className="p-4 sm:p-5 border-t border-border/60 flex items-center justify-end gap-2">
          <button onClick={onClose} className="px-3 py-2 rounded-lg bg-muted text-foreground hover:bg-muted/80">{t('common.cancel')}</button>
          <button onClick={save} disabled={saving} className="px-3 py-2 rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50">{saving ? t('common.saving') : t('common.save')}</button>
        </div>
      </div>
    </div>
  );
};

export default UserSettingsPanel;
