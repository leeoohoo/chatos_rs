// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../i18n/I18nProvider';
import { useApiClient } from '../lib/api/ApiClientContext';
import { useChatRuntimeEnv } from '../lib/store/ChatStoreContext';
import AiModelManager from './AiModelManager';
import { CloudAiSettingsSection } from './settings/CloudAiSettingsSection';
import {
  GeneralSettingsSection,
  normalizeLocale,
  type UserPreferences,
} from './settings/GeneralSettingsSection';

interface Props {
  onClose: () => void;
}

const UserSettingsPanel: React.FC<Props> = ({ onClose }) => {
  const client = useApiClient();
  const { userId } = useChatRuntimeEnv();
  const { locale, setLocale, t } = useI18n();
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [notice, setNotice] = React.useState<string | null>(null);
  const [activeSection, setActiveSection] = React.useState<'general' | 'cloud_ai'>('general');
  const [showAiModelManager, setShowAiModelManager] = React.useState(false);
  const [cloudAiRefreshKey, setCloudAiRefreshKey] = React.useState(0);
  const [preferences, setPreferences] = React.useState<UserPreferences>({
    INTERNAL_CONTEXT_LOCALE: 'zh-CN',
    UI_LOCALE: normalizeLocale(locale),
  });

  const getErrorMessage = React.useCallback((value: unknown): string => {
    if (value instanceof Error) return value.message;
    if (typeof value === 'string') return value;
    return t('common.unknown');
  }, [t]);

  React.useEffect(() => {
    let mounted = true;
    setLoading(true);
    setError(null);
    void client.getUserSettings(userId)
      .then((response) => {
        if (!mounted) return;
        const effective = response?.effective || {};
        setPreferences({
          INTERNAL_CONTEXT_LOCALE: normalizeLocale(effective.INTERNAL_CONTEXT_LOCALE),
          UI_LOCALE: normalizeLocale(effective.UI_LOCALE),
        });
      })
      .catch((value: unknown) => {
        if (mounted) setError(getErrorMessage(value));
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });
    return () => {
      mounted = false;
    };
  }, [client, getErrorMessage, userId]);

  const save = async () => {
    if (!userId) {
      setError(t('settings.missingUserId'));
      return;
    }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const response = await client.updateUserSettings(userId, { ...preferences });
      const effective = response?.effective || preferences;
      const nextPreferences = {
        INTERNAL_CONTEXT_LOCALE: normalizeLocale(effective.INTERNAL_CONTEXT_LOCALE),
        UI_LOCALE: normalizeLocale(effective.UI_LOCALE),
      };
      setPreferences(nextPreferences);
      setLocale(nextPreferences.UI_LOCALE);
      setNotice(t('settings.saved'));
    } catch (value: unknown) {
      setError(getErrorMessage(value));
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
      <div className="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-6">
        <div className="absolute inset-0 bg-gradient-to-b from-background/60 to-background/80 backdrop-blur-sm" />
        <div className="relative flex max-h-[88vh] w-full max-w-6xl flex-col rounded-xl border border-border/60 bg-card text-card-foreground shadow-2xl">
          <div className="flex items-center justify-between border-b border-border/60 p-4 sm:p-5">
            <div className="flex items-center gap-3">
              <div className="rounded-lg bg-accent/60 p-2 text-accent-foreground">
                <svg className="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6V4m0 16v-2m8-6h2M4 12H2m15.364 5.364l1.414 1.414M5.636 6.636L4.222 5.222m12.728 0l1.414 1.414M5.636 17.364l-1.414 1.414" />
                </svg>
              </div>
              <div>
                <h3 className="font-semibold leading-tight">{t('settings.title')}</h3>
                <p className="mt-0.5 text-xs text-muted-foreground">{t('settings.subtitle')}</p>
              </div>
            </div>
            <button onClick={onClose} className="rounded-lg p-2 transition-colors hover:bg-accent" aria-label={t('common.close')}>
              <svg className="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <div className="flex gap-2 border-b border-border/60 px-4 py-2 sm:px-5">
            <SettingsTab
              active={activeSection === 'general'}
              label={t('settings.section.general')}
              onClick={() => setActiveSection('general')}
            />
            <SettingsTab
              active={activeSection === 'cloud_ai'}
              label={t('settings.section.cloudAi')}
              onClick={() => setActiveSection('cloud_ai')}
            />
          </div>

          <div className="flex-1 overflow-y-auto p-4 sm:p-6">
            {activeSection === 'general' ? (
              <GeneralSettingsSection
                loading={loading}
                error={error}
                notice={notice}
                preferences={preferences}
                setPreferences={setPreferences}
              />
            ) : (
              <CloudAiSettingsSection
                refreshKey={cloudAiRefreshKey}
                onManageModels={() => setShowAiModelManager(true)}
              />
            )}
          </div>

          <div className="flex items-center justify-end gap-2 border-t border-border/60 p-4 sm:p-5">
            <button onClick={onClose} className="rounded-lg bg-muted px-3 py-2 text-foreground hover:bg-muted/80">
              {activeSection === 'general' ? t('common.cancel') : t('common.close')}
            </button>
            {activeSection === 'general' ? (
              <button onClick={save} disabled={loading || saving} className="rounded-lg bg-primary px-3 py-2 text-primary-foreground hover:bg-primary/90 disabled:opacity-50">
                {saving ? t('common.saving') : t('common.save')}
              </button>
            ) : null}
          </div>
        </div>
      </div>
      {showAiModelManager ? (
        <AiModelManager
          onClose={() => {
            setShowAiModelManager(false);
            setCloudAiRefreshKey((value) => value + 1);
          }}
        />
      ) : null}
    </>
  );
};

function SettingsTab({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`rounded-lg px-3 py-2 text-sm transition-colors ${
        active ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:bg-accent'
      }`}
    >
      {label}
    </button>
  );
}

export default UserSettingsPanel;
