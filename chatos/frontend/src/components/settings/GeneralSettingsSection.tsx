// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';

export interface UserPreferences {
  INTERNAL_CONTEXT_LOCALE: 'zh-CN' | 'en-US';
  UI_LOCALE: 'zh-CN' | 'en-US';
}

export const normalizeLocale = (value: unknown): 'zh-CN' | 'en-US' => (
  value === 'en-US' ? 'en-US' : 'zh-CN'
);

export function GeneralSettingsSection({
  loading,
  error,
  notice,
  preferences,
  setPreferences,
}: {
  loading: boolean;
  error: string | null;
  notice: string | null;
  preferences: UserPreferences;
  setPreferences: React.Dispatch<React.SetStateAction<UserPreferences>>;
}) {
  const { t } = useI18n();
  if (loading) return <div className="text-sm text-muted-foreground">{t('common.loading')}</div>;
  return (
    <div className="mx-auto max-w-2xl space-y-4">
      {error ? <div className="rounded-lg border border-destructive/20 bg-destructive/10 p-2 text-sm text-destructive">{error}</div> : null}
      {notice ? <div className="rounded-lg border border-primary/20 bg-primary/10 p-2 text-sm text-primary">{notice}</div> : null}
      <div className="overflow-hidden rounded-xl border border-border/60">
        <div className="border-b border-border/60 bg-accent/10 px-4 py-2.5 text-sm font-medium">{t('settings.section.language')}</div>
        <div className="grid grid-cols-1 gap-4 p-4 sm:grid-cols-2">
          <LocaleSelect
            label={t('settings.uiLocale')}
            help={t('settings.uiLocaleHelp')}
            value={preferences.UI_LOCALE}
            onChange={(value) => setPreferences((current) => ({ ...current, UI_LOCALE: value }))}
          />
          <LocaleSelect
            label={t('settings.internalContextLocale')}
            help={t('settings.internalContextLocaleHelp')}
            value={preferences.INTERNAL_CONTEXT_LOCALE}
            onChange={(value) => setPreferences((current) => ({ ...current, INTERNAL_CONTEXT_LOCALE: value }))}
          />
        </div>
      </div>
    </div>
  );
}

function LocaleSelect({
  label,
  help,
  value,
  onChange,
}: {
  label: string;
  help: string;
  value: 'zh-CN' | 'en-US';
  onChange: (value: 'zh-CN' | 'en-US') => void;
}) {
  const { t } = useI18n();
  return (
    <div>
      <label className="text-xs text-muted-foreground">{label}</label>
      <select
        className="mt-1 w-full rounded-lg border bg-background p-2 focus:outline-none focus:ring-2 focus:ring-primary/40"
        value={value}
        onChange={(event) => onChange(normalizeLocale(event.target.value))}
      >
        <option value="zh-CN">{t('language.chinese')}</option>
        <option value="en-US">{t('language.english')}</option>
      </select>
      <p className="mt-1 text-[11px] text-muted-foreground">{help}</p>
    </div>
  );
}
