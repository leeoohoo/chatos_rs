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
    <div className="mx-3 mb-3 rounded-xl border border-border bg-card p-3 shadow-sm">
      <div className="mb-2">
        <div className="text-sm font-semibold text-foreground">
          {panel.title || '需要你的输入'}
        </div>
        {panel.message ? (
          <div className="mt-1 text-xs text-muted-foreground">
            {panel.message}
          </div>
        ) : null}
      </div>

      {fields.length > 0 ? (
        <div className="custom-scrollbar max-h-52 space-y-2 overflow-y-scroll pr-1 [scrollbar-gutter:stable]">
          {fields.map((field) => (
            <label key={field.key} className="flex flex-col gap-1 text-xs text-muted-foreground">
              <span>
                {field.label || field.key}
                {field.required ? <span className="ml-1 text-destructive">*</span> : null}
              </span>
              {field.description ? (
                <span className="text-[11px] text-muted-foreground">{field.description}</span>
              ) : null}
              {field.multiline ? (
                <textarea
                  value={values[field.key] || ''}
                  onChange={(event) => setValues((prev) => ({ ...prev, [field.key]: event.target.value }))}
                  className="min-h-[64px] rounded-md border border-border bg-background px-2 py-1 text-sm text-foreground outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-2 focus:ring-ring/20"
                  placeholder={field.placeholder || ''}
                  disabled={panel.submitting === true}
                />
              ) : (
                <input
                  type={field.secret ? 'password' : 'text'}
                  value={values[field.key] || ''}
                  onChange={(event) => setValues((prev) => ({ ...prev, [field.key]: event.target.value }))}
                  className="rounded-md border border-border bg-background px-2 py-1 text-sm text-foreground outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-2 focus:ring-ring/20"
                  placeholder={field.placeholder || ''}
                  disabled={panel.submitting === true}
                />
              )}
            </label>
          ))}
        </div>
      ) : null}

      {hasChoice ? (
        <div className="mt-3 rounded-lg border border-border bg-background/50 p-2">
          <div className="mb-2 text-xs font-medium text-muted-foreground">
            请选择{isMultiple ? `（${minSelections}-${maxSelections}项）` : ''}
          </div>
          <div className="custom-scrollbar max-h-72 space-y-1.5 overflow-y-scroll pr-1 [scrollbar-gutter:stable]">
            {choice!.options.map((option) => {
              const checked = isMultiple
                ? multiSelection.includes(option.value)
                : singleSelection === option.value;
              return (
                <label key={option.value} className="flex cursor-pointer items-start gap-2 text-sm text-foreground">
                  <input
                    type={isMultiple ? 'checkbox' : 'radio'}
                    name={`ui_prompt_${panel.promptId}`}
                    className="mt-0.5 h-4 w-4 accent-primary"
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
                      <span className="block text-xs text-muted-foreground">{option.description}</span>
                    ) : null}
                  </span>
                </label>
              );
            })}
          </div>
        </div>
      ) : null}

      {panel.error ? (
        <div className="mt-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs text-destructive">
          {panel.error}
        </div>
      ) : null}

      {selectionInvalid ? (
        <div className="mt-2 text-xs text-destructive">
          选择项数量不符合限制（最少 {minSelections}，最多 {maxSelections}）。
        </div>
      ) : null}

      <div className="mt-3 flex items-center justify-end gap-2">
        {panel.allowCancel !== false ? (
          <button
            type="button"
            className="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60"
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
