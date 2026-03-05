import React, { useEffect, useMemo, useState } from 'react';

import type {
  UiPromptChoice,
  UiPromptField,
  UiPromptPanelState,
  UiPromptResponsePayload,
} from '../lib/store/types';

interface UiPromptPanelProps {
  panel: UiPromptPanelState;
  onSubmit: (payload: UiPromptResponsePayload) => Promise<void> | void;
  onCancel: () => Promise<void> | void;
}

const buildInitialValues = (fields: UiPromptField[]): Record<string, string> => {
  const out: Record<string, string> = {};
  fields.forEach((field) => {
    out[field.key] = typeof field.default === 'string' ? field.default : '';
  });
  return out;
};

const normalizeSingleSelection = (choice?: UiPromptChoice): string => {
  if (!choice || choice.multiple === true) {
    return '';
  }
  const rawDefault = choice.default;
  const defaultValue = typeof rawDefault === 'string'
    ? rawDefault
    : (Array.isArray(rawDefault) ? String(rawDefault[0] || '') : '');
  if (choice.options.some((option) => option.value === defaultValue)) {
    return defaultValue;
  }
  return '';
};

const normalizeMultiSelection = (choice?: UiPromptChoice): string[] => {
  if (!choice || choice.multiple !== true) {
    return [];
  }
  const rawDefault = choice.default;
  const source = Array.isArray(rawDefault)
    ? rawDefault
    : (typeof rawDefault === 'string' && rawDefault ? [rawDefault] : []);
  const allowed = new Set(choice.options.map((option) => option.value));
  const seen = new Set<string>();
  const out: string[] = [];
  source.forEach((item) => {
    const value = String(item || '').trim();
    if (!value || !allowed.has(value) || seen.has(value)) {
      return;
    }
    seen.add(value);
    out.push(value);
  });
  return out;
};

export const UiPromptPanel: React.FC<UiPromptPanelProps> = ({ panel, onSubmit, onCancel }) => {
  const fields = Array.isArray(panel.payload?.fields) ? panel.payload.fields : [];
  const choice = panel.payload?.choice;
  const [values, setValues] = useState<Record<string, string>>(() => buildInitialValues(fields));
  const [singleSelection, setSingleSelection] = useState<string>(() => normalizeSingleSelection(choice));
  const [multiSelection, setMultiSelection] = useState<string[]>(() => normalizeMultiSelection(choice));

  useEffect(() => {
    setValues(buildInitialValues(fields));
    setSingleSelection(normalizeSingleSelection(choice));
    setMultiSelection(normalizeMultiSelection(choice));
  }, [panel.promptId, fields, choice]);

  const hasChoice = !!choice && Array.isArray(choice.options) && choice.options.length > 0;
  const isMultiple = hasChoice && choice?.multiple === true;
  const minSelections = hasChoice
    ? Math.max(0, Number(choice?.min_selections ?? (isMultiple ? 0 : 0)))
    : 0;
  const maxSelections = hasChoice
    ? Math.max(0, Number(choice?.max_selections ?? (isMultiple ? choice?.options?.length || 0 : 1)))
    : 0;

  const requiredFieldMissing = useMemo(() => (
    fields.some((field) => {
      if (!field.required) {
        return false;
      }
      return !(values[field.key] || '').trim();
    })
  ), [fields, values]);

  const selectionCount = hasChoice
    ? (isMultiple ? multiSelection.length : (singleSelection ? 1 : 0))
    : 0;
  const selectionInvalid = hasChoice
    ? (selectionCount < minSelections || selectionCount > maxSelections)
    : false;

  const submitDisabled = panel.submitting === true || requiredFieldMissing || selectionInvalid;

  const handleSubmit = async () => {
    if (submitDisabled) {
      return;
    }

    const payload: UiPromptResponsePayload = {
      status: 'ok',
    };

    if (fields.length > 0) {
      payload.values = { ...values };
    }
    if (hasChoice) {
      payload.selection = isMultiple ? [...multiSelection] : singleSelection;
    }

    await onSubmit(payload);
  };

  const handleCancel = async () => {
    if (panel.submitting === true || panel.allowCancel === false) {
      return;
    }
    await onCancel();
  };

  return (
    <div className="mx-3 mb-3 rounded-xl border border-blue-300 bg-blue-50 p-3 shadow-sm dark:border-blue-700 dark:bg-blue-900/20">
      <div className="mb-2">
        <div className="text-sm font-semibold text-blue-900 dark:text-blue-100">
          {panel.title || '需要你的输入'}
        </div>
        {panel.message ? (
          <div className="mt-1 text-xs text-blue-800/90 dark:text-blue-200/90">
            {panel.message}
          </div>
        ) : null}
      </div>

      {fields.length > 0 ? (
        <div className="space-y-2">
          {fields.map((field) => (
            <label key={field.key} className="flex flex-col gap-1 text-xs text-slate-600 dark:text-slate-300">
              <span>
                {field.label || field.key}
                {field.required ? <span className="ml-1 text-red-500">*</span> : null}
              </span>
              {field.description ? (
                <span className="text-[11px] text-slate-500 dark:text-slate-400">{field.description}</span>
              ) : null}
              {field.multiline ? (
                <textarea
                  value={values[field.key] || ''}
                  onChange={(event) => setValues((prev) => ({ ...prev, [field.key]: event.target.value }))}
                  className="min-h-[64px] rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  placeholder={field.placeholder || ''}
                  disabled={panel.submitting === true}
                />
              ) : (
                <input
                  type={field.secret ? 'password' : 'text'}
                  value={values[field.key] || ''}
                  onChange={(event) => setValues((prev) => ({ ...prev, [field.key]: event.target.value }))}
                  className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  placeholder={field.placeholder || ''}
                  disabled={panel.submitting === true}
                />
              )}
            </label>
          ))}
        </div>
      ) : null}

      {hasChoice ? (
        <div className="mt-3 rounded-lg border border-blue-200 bg-white p-2 dark:border-blue-800 dark:bg-slate-900/60">
          <div className="mb-2 text-xs font-medium text-slate-600 dark:text-slate-300">
            请选择{isMultiple ? `（${minSelections}-${maxSelections}项）` : ''}
          </div>
          <div className="space-y-1.5">
            {choice!.options.map((option) => {
              const checked = isMultiple
                ? multiSelection.includes(option.value)
                : singleSelection === option.value;
              return (
                <label key={option.value} className="flex cursor-pointer items-start gap-2 text-sm text-slate-700 dark:text-slate-200">
                  <input
                    type={isMultiple ? 'checkbox' : 'radio'}
                    name={`ui_prompt_${panel.promptId}`}
                    checked={checked}
                    onChange={(event) => {
                      if (isMultiple) {
                        setMultiSelection((prev) => {
                          if (event.target.checked) {
                            if (prev.includes(option.value)) {
                              return prev;
                            }
                            return [...prev, option.value];
                          }
                          return prev.filter((item) => item !== option.value);
                        });
                      } else {
                        setSingleSelection(event.target.checked ? option.value : '');
                      }
                    }}
                    disabled={panel.submitting === true}
                  />
                  <span>
                    <span>{option.label || option.value}</span>
                    {option.description ? (
                      <span className="block text-xs text-slate-500 dark:text-slate-400">{option.description}</span>
                    ) : null}
                  </span>
                </label>
              );
            })}
          </div>
        </div>
      ) : null}

      {panel.error ? (
        <div className="mt-2 rounded-md border border-red-300 bg-red-50 px-2 py-1 text-xs text-red-700 dark:border-red-800 dark:bg-red-950/30 dark:text-red-200">
          {panel.error}
        </div>
      ) : null}

      {selectionInvalid ? (
        <div className="mt-2 text-xs text-amber-700 dark:text-amber-300">
          选择项数量不符合限制（最少 {minSelections}，最多 {maxSelections}）。
        </div>
      ) : null}

      <div className="mt-3 flex items-center justify-end gap-2">
        {panel.allowCancel !== false ? (
          <button
            type="button"
            className="rounded-md border border-slate-300 px-3 py-1.5 text-sm text-slate-700 hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
            onClick={handleCancel}
            disabled={panel.submitting === true}
          >
            取消
          </button>
        ) : null}
        <button
          type="button"
          className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={handleSubmit}
          disabled={submitDisabled}
        >
          {panel.submitting ? '提交中...' : '确认提交'}
        </button>
      </div>
    </div>
  );
};

export default UiPromptPanel;
