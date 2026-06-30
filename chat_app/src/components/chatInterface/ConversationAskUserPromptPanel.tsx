import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { Check, Loader2, X } from 'lucide-react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import type {
  AskUserPromptChoicePayload,
  AskUserPromptFieldPayload,
  AskUserPromptRecord,
  AskUserPromptStoredPrompt,
} from '../../lib/api/client/types';
import { useConversationAskUserPromptRealtime } from '../../lib/realtime/useConversationAskUserPromptRealtime';

interface ConversationAskUserPromptPanelProps {
  sessionId: string | null;
  projectId?: string | null;
}

const isPendingPrompt = (prompt: AskUserPromptRecord): boolean => (
  String(prompt.status || '').trim().toLowerCase() === 'pending'
);

const asRecordPrompt = (prompt: AskUserPromptRecord): AskUserPromptStoredPrompt => (
  prompt.prompt && typeof prompt.prompt === 'object' ? prompt.prompt : {}
);

const fieldsFromPrompt = (prompt: AskUserPromptRecord | null): AskUserPromptFieldPayload[] => {
  if (!prompt) {
    return [];
  }
  const payload = asRecordPrompt(prompt).payload;
  return Array.isArray(payload?.fields) ? payload.fields : [];
};

const choiceFromPrompt = (prompt: AskUserPromptRecord | null): AskUserPromptChoicePayload | null => {
  if (!prompt) {
    return null;
  }
  const choice = asRecordPrompt(prompt).payload?.choice;
  return choice && typeof choice === 'object' ? choice : null;
};

const fieldKey = (field: AskUserPromptFieldPayload, index: number): string => {
  const explicit = String(field.key || field.name || '').trim();
  if (explicit) {
    return explicit;
  }
  const label = String(field.label || '').trim();
  if (!label) {
    return `field_${index + 1}`;
  }
  return label
    .toLowerCase()
    .replace(/[^a-z0-9_\u4e00-\u9fa5]+/gi, '_')
    .replace(/^_+|_+$/g, '')
    || `field_${index + 1}`;
};

const fieldDefaultValue = (field: AskUserPromptFieldPayload): string => (
  String(field.default_value ?? field.default ?? '')
);

const choiceOptions = (choice: AskUserPromptChoicePayload | null) => (
  Array.isArray(choice?.options) ? choice.options : []
);

const defaultSelection = (choice: AskUserPromptChoicePayload | null): string[] => {
  const raw = choice?.default;
  if (Array.isArray(raw)) {
    return raw.map((item) => String(item).trim()).filter(Boolean);
  }
  const value = String(raw || '').trim();
  return value ? [value] : [];
};

const promptTitle = (prompt: AskUserPromptRecord): string => {
  const stored = asRecordPrompt(prompt);
  return String(stored.title || '').trim();
};

const promptMessage = (prompt: AskUserPromptRecord): string => {
  const stored = asRecordPrompt(prompt);
  return String(stored.message || '').trim();
};

const promptAllowCancel = (prompt: AskUserPromptRecord): boolean => (
  asRecordPrompt(prompt).allow_cancel !== false
);

const ConversationAskUserPromptPanel: React.FC<ConversationAskUserPromptPanelProps> = ({
  sessionId,
  projectId,
}) => {
  const apiClient = useApiClient();
  const { t } = useI18n();
  const [prompts, setPrompts] = useState<AskUserPromptRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [values, setValues] = useState<Record<string, string>>({});
  const [selection, setSelection] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const loadPrompts = useCallback(async () => {
    const normalizedSessionId = String(sessionId || '').trim();
    if (!normalizedSessionId) {
      setPrompts([]);
      return;
    }
    setLoading(true);
    try {
      const result = await apiClient.listAskUserPrompts(normalizedSessionId, {
        includePending: true,
        limit: 100,
      });
      setPrompts((result.prompts || []).filter(isPendingPrompt));
    } catch (loadError) {
      console.error('Failed to load ask user prompts:', loadError);
      setError(t('askUserPrompt.loadFailed'));
    } finally {
      setLoading(false);
    }
  }, [apiClient, sessionId, t]);

  useEffect(() => {
    void loadPrompts();
  }, [loadPrompts]);

  useConversationAskUserPromptRealtime({
    sessionId,
    projectId,
    enabled: Boolean(sessionId),
    onEvent: async () => {
      await loadPrompts();
    },
  });

  const activePrompt = useMemo(
    () => prompts.find(isPendingPrompt) || null,
    [prompts],
  );
  const activeFields = useMemo(() => fieldsFromPrompt(activePrompt), [activePrompt]);
  const activeChoice = useMemo(() => choiceFromPrompt(activePrompt), [activePrompt]);
  const activeOptions = useMemo(() => choiceOptions(activeChoice), [activeChoice]);
  const multipleChoice = activeChoice?.multiple === true;
  const hasChoice = activeOptions.length > 0;

  useEffect(() => {
    if (!activePrompt) {
      setValues({});
      setSelection([]);
      setError(null);
      return;
    }
    const nextValues: Record<string, string> = {};
    activeFields.forEach((field, index) => {
      nextValues[fieldKey(field, index)] = fieldDefaultValue(field);
    });
    setValues(nextValues);
    setSelection(defaultSelection(activeChoice));
    setError(null);
  }, [activePrompt, activeFields, activeChoice]);

  const validate = useCallback((): string | null => {
    for (let index = 0; index < activeFields.length; index += 1) {
      const field = activeFields[index];
      if (field.required !== true) {
        continue;
      }
      const key = fieldKey(field, index);
      if (!String(values[key] || '').trim()) {
        return t('askUserPrompt.fieldRequired', {
          field: String(field.label || key),
        });
      }
    }

    if (hasChoice) {
      const min = Number(activeChoice?.min_selections ?? 0);
      const max = Number(activeChoice?.max_selections ?? (multipleChoice ? activeOptions.length : 1));
      const selectedCount = selection.filter(Boolean).length;
      if (selectedCount < min || selectedCount > max) {
        return t('askUserPrompt.selectionInvalid', { min, max });
      }
    }

    return null;
  }, [
    activeChoice,
    activeFields,
    activeOptions.length,
    hasChoice,
    multipleChoice,
    selection,
    t,
    values,
  ]);

  const handleSubmit = useCallback(async () => {
    if (!activePrompt || !sessionId) {
      return;
    }
    const nextError = validate();
    if (nextError) {
      setError(nextError);
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      const response = await apiClient.submitAskUserPrompt(activePrompt.id, {
        conversation_id: sessionId,
        values: activeFields.length > 0 ? values : undefined,
        selection: hasChoice
          ? (multipleChoice ? selection : (selection[0] || ''))
          : undefined,
      });
      if (response?.success === false) {
        throw new Error(response.error || t('askUserPrompt.submitFailed'));
      }
      await loadPrompts();
    } catch (submitError) {
      setError(submitError instanceof Error ? submitError.message : t('askUserPrompt.submitFailed'));
    } finally {
      setSubmitting(false);
    }
  }, [
    activeFields.length,
    activePrompt,
    apiClient,
    hasChoice,
    loadPrompts,
    multipleChoice,
    selection,
    sessionId,
    t,
    validate,
    values,
  ]);

  const handleCancel = useCallback(async () => {
    if (!activePrompt || !sessionId || !promptAllowCancel(activePrompt)) {
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      const response = await apiClient.cancelAskUserPrompt(activePrompt.id, {
        conversation_id: sessionId,
        reason: 'user_cancelled',
      });
      if (response?.success === false) {
        throw new Error(response.error || t('askUserPrompt.cancelFailed'));
      }
      await loadPrompts();
    } catch (cancelError) {
      setError(cancelError instanceof Error ? cancelError.message : t('askUserPrompt.cancelFailed'));
    } finally {
      setSubmitting(false);
    }
  }, [activePrompt, apiClient, loadPrompts, sessionId, t]);

  const updateFieldValue = useCallback((key: string, nextValue: string) => {
    setValues((prev) => ({
      ...prev,
      [key]: nextValue,
    }));
  }, []);

  const toggleChoice = useCallback((value: string) => {
    setSelection((prev) => {
      if (!multipleChoice) {
        return [value];
      }
      if (prev.includes(value)) {
        return prev.filter((item) => item !== value);
      }
      return [...prev, value];
    });
  }, [multipleChoice]);

  if (!sessionId || (!activePrompt && !loading)) {
    return null;
  }

  if (!activePrompt) {
    return (
      <div className="border-t border-border bg-background/95 px-4 py-3">
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t('askUserPrompt.loading')}
        </div>
      </div>
    );
  }

  const title = promptTitle(activePrompt) || t('askUserPrompt.titleFallback');
  const message = promptMessage(activePrompt);
  const allowCancel = promptAllowCancel(activePrompt);
  const pendingCount = prompts.length;

  return (
    <div className="border-t border-border bg-background/95 px-4 py-3">
      <div className="rounded-md border border-primary/30 bg-primary/5 p-4">
        <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="text-sm font-semibold text-foreground">
                {title}
              </h3>
              <span className="rounded-full border border-primary/30 px-2 py-0.5 text-xs text-primary">
                {t('askUserPrompt.pendingCount', { count: pendingCount })}
              </span>
            </div>
            {message ? (
              <p className="mt-1 text-sm text-muted-foreground">
                {message}
              </p>
            ) : null}
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {allowCancel ? (
              <button
                type="button"
                onClick={handleCancel}
                disabled={submitting}
                title={t('common.cancel')}
                className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-border bg-background text-muted-foreground transition-colors hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60"
              >
                <X className="h-4 w-4" />
              </button>
            ) : null}
            <button
              type="button"
              onClick={handleSubmit}
              disabled={submitting}
              title={t('askUserPrompt.submit')}
              className="inline-flex h-8 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-60"
            >
              {submitting ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Check className="h-4 w-4" />
              )}
              <span>{t('askUserPrompt.submit')}</span>
            </button>
          </div>
        </div>

        {activeFields.length > 0 ? (
          <div className="mt-4 grid gap-3 md:grid-cols-2">
            {activeFields.map((field, index) => {
              const key = fieldKey(field, index);
              const label = String(field.label || key);
              const commonClassName = "w-full rounded-md border border-input bg-background px-3 py-2 text-sm text-foreground outline-none transition-colors focus:border-primary";
              return (
                <label key={key} className="block min-w-0">
                  <span className="mb-1 block text-xs font-medium text-muted-foreground">
                    {label}
                    {field.required === true ? <span className="text-destructive"> *</span> : null}
                  </span>
                  {field.multiline === true ? (
                    <textarea
                      value={values[key] || ''}
                      placeholder={field.placeholder}
                      rows={3}
                      onChange={(event) => updateFieldValue(key, event.target.value)}
                      className={commonClassName}
                    />
                  ) : (
                    <input
                      type={field.secret === true ? 'password' : 'text'}
                      value={values[key] || ''}
                      placeholder={field.placeholder}
                      onChange={(event) => updateFieldValue(key, event.target.value)}
                      className={commonClassName}
                    />
                  )}
                </label>
              );
            })}
          </div>
        ) : null}

        {hasChoice ? (
          <div className="mt-4">
            <div className="mb-2 text-xs font-medium text-muted-foreground">
              {t('askUserPrompt.choose', { suffix: multipleChoice ? t('askUserPrompt.multipleSuffix') : '' })}
            </div>
            <div className="grid gap-2 md:grid-cols-2">
              {activeOptions.map((option) => {
                const value = String(option.value || '').trim();
                if (!value) {
                  return null;
                }
                const checked = selection.includes(value);
                return (
                  <label
                    key={value}
                    className="flex min-h-11 cursor-pointer items-start gap-2 rounded-md border border-border bg-background px-3 py-2 text-sm transition-colors hover:bg-accent"
                  >
                    <input
                      type={multipleChoice ? 'checkbox' : 'radio'}
                      checked={checked}
                      onChange={() => toggleChoice(value)}
                      className="mt-0.5 h-4 w-4 accent-primary"
                    />
                    <span className="min-w-0">
                      <span className="block font-medium text-foreground">
                        {option.label || value}
                      </span>
                      {option.description ? (
                        <span className="block text-xs text-muted-foreground">
                          {option.description}
                        </span>
                      ) : null}
                    </span>
                  </label>
                );
              })}
            </div>
          </div>
        ) : null}

        {error ? (
          <div className="mt-3 text-sm text-destructive">
            {error}
          </div>
        ) : null}
      </div>
    </div>
  );
};

export default ConversationAskUserPromptPanel;
