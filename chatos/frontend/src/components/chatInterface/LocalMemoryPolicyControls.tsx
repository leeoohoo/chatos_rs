// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useEffect, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import { isLocalRuntimeSessionId } from '../../lib/api/localRuntime';
import { LocalMemoryPolicyNumberInput } from './LocalMemoryPolicyNumberInput';

interface LocalMemoryPolicy {
  enabled: boolean;
  messageThreshold: number;
  characterThreshold: number;
  recallLimit: number;
}

const DEFAULT_POLICY: LocalMemoryPolicy = {
  enabled: true,
  messageThreshold: 24,
  characterThreshold: 32_000,
  recallLimit: 8,
};

export const LocalMemoryPolicyControls: React.FC<{ sessionId: string }> = ({ sessionId }) => {
  const client = useApiClient();
  const { t } = useI18n();
  const [policy, setPolicy] = useState(DEFAULT_POLICY);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (!isLocalRuntimeSessionId(sessionId)) {
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    void client.getConversationRuntimeSettings(sessionId)
      .then((settings) => {
        if (cancelled) return;
        setPolicy({
          enabled: settings.memory_auto_summary_enabled !== false,
          messageThreshold: normalizeInteger(
            settings.memory_summary_message_threshold,
            DEFAULT_POLICY.messageThreshold,
          ),
          characterThreshold: normalizeInteger(
            settings.memory_summary_character_threshold,
            DEFAULT_POLICY.characterThreshold,
          ),
          recallLimit: normalizeInteger(
            settings.memory_recall_limit,
            DEFAULT_POLICY.recallLimit,
          ),
        });
      })
      .catch((loadError) => {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : t('memory.policy.loadFailed'));
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionId, t]);

  if (!isLocalRuntimeSessionId(sessionId)) {
    return null;
  }

  const save = async () => {
    setSaving(true);
    setSaved(false);
    setError(null);
    try {
      const response = await client.updateConversationRuntimeSettings(sessionId, {
        memory_auto_summary_enabled: policy.enabled,
        memory_summary_message_threshold: policy.messageThreshold,
        memory_summary_character_threshold: policy.characterThreshold,
        memory_recall_limit: policy.recallLimit,
      });
      setPolicy({
        enabled: response.memory_auto_summary_enabled !== false,
        messageThreshold: normalizeInteger(
          response.memory_summary_message_threshold,
          DEFAULT_POLICY.messageThreshold,
        ),
        characterThreshold: normalizeInteger(
          response.memory_summary_character_threshold,
          DEFAULT_POLICY.characterThreshold,
        ),
        recallLimit: normalizeInteger(
          response.memory_recall_limit,
          DEFAULT_POLICY.recallLimit,
        ),
      });
      setSaved(true);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : t('memory.policy.saveFailed'));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="rounded-lg border border-border bg-background/80 p-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-xs font-semibold">{t('memory.policy.title')}</div>
          <div className="mt-1 text-[11px] text-muted-foreground">
            {t('memory.policy.description')}
          </div>
        </div>
        <input
          type="checkbox"
          checked={policy.enabled}
          disabled={loading || saving}
          aria-label={t('memory.policy.enabled')}
          onChange={(event) => setPolicy((current) => ({
            ...current,
            enabled: event.target.checked,
          }))}
        />
      </div>
      <div className="mt-3 grid grid-cols-3 gap-2">
        <LocalMemoryPolicyNumberInput
          label={t('memory.policy.messageThreshold')}
          value={policy.messageThreshold}
          disabled={loading || saving || !policy.enabled}
          min={4}
          max={2_000}
          onChange={(value) => setPolicy((current) => ({ ...current, messageThreshold: value }))}
        />
        <LocalMemoryPolicyNumberInput
          label={t('memory.policy.characterThreshold')}
          value={policy.characterThreshold}
          disabled={loading || saving || !policy.enabled}
          min={4_000}
          max={2_000_000}
          onChange={(value) => setPolicy((current) => ({ ...current, characterThreshold: value }))}
        />
        <LocalMemoryPolicyNumberInput
          label={t('memory.policy.recallLimit')}
          value={policy.recallLimit}
          disabled={loading || saving}
          min={2}
          max={50}
          onChange={(value) => setPolicy((current) => ({ ...current, recallLimit: value }))}
        />
      </div>
      <div className="mt-3 flex items-center justify-between gap-2">
        <div className="text-[11px] text-muted-foreground">
          {error || (saved ? t('memory.policy.saved') : '')}
        </div>
        <button
          type="button"
          className="px-2 py-1 text-xs rounded border border-border hover:bg-accent disabled:opacity-60"
          disabled={loading || saving}
          onClick={() => void save()}
        >
          {saving ? t('common.saving') : t('common.save')}
        </button>
      </div>
    </div>
  );
};

const normalizeInteger = (value: unknown, fallback: number): number => {
  const numeric = Number(value);
  return Number.isFinite(numeric) && numeric > 0 ? Math.trunc(numeric) : fallback;
};
