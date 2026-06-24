import type { AskUserPromptRecord } from '../../types';

export interface PromptField {
  key: string;
  label: string;
  description?: string;
  placeholder?: string;
  default?: string;
  required?: boolean;
  multiline?: boolean;
  secret?: boolean;
}

export interface PromptChoiceOption {
  value: string;
  label?: string;
  description?: string;
}

export interface PromptChoice {
  multiple?: boolean;
  options: PromptChoiceOption[];
  default?: unknown;
  min_selections?: number;
  max_selections?: number;
}

export function buildInitialValues(prompt: AskUserPromptRecord): Record<string, unknown> {
  const values: Record<string, unknown> = {};
  extractFields(prompt).forEach((field) => {
    values[field.key] = field.default ?? '';
  });

  const choice = extractChoice(prompt);
  if (choice) {
    values.selection =
      prompt.response?.selection ??
      choice.default ??
      (choice.multiple ? [] : '');
  }

  const responseValues = asRecord(prompt.response?.values);
  if (responseValues) {
    Object.assign(values, responseValues);
  }

  return values;
}

export function extractFields(prompt: AskUserPromptRecord): PromptField[] {
  const payload = asRecord(prompt.payload);
  const rawFields = Array.isArray(payload?.fields) ? payload.fields : [];
  return rawFields
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => Boolean(item))
    .map((field) => ({
      key: asString(field.key) || 'field',
      label: asString(field.label) || asString(field.key) || 'field',
      description: asOptionalString(field.description),
      placeholder: asOptionalString(field.placeholder),
      default: asOptionalString(field.default) ?? '',
      required: Boolean(field.required),
      multiline: Boolean(field.multiline),
      secret: Boolean(field.secret),
    }));
}

export function extractChoice(prompt: AskUserPromptRecord): PromptChoice | null {
  const payload = asRecord(prompt.payload);
  const choice = asRecord(payload?.choice);
  if (!choice || !Array.isArray(choice.options) || choice.options.length === 0) {
    return null;
  }

  return {
    multiple: Boolean(choice.multiple),
    default: choice.default,
    min_selections: asNumber(choice.min_selections),
    max_selections: asNumber(choice.max_selections),
    options: choice.options
      .map((item) => asRecord(item))
      .filter((item): item is Record<string, unknown> => Boolean(item))
      .map((option) => ({
        value: asString(option.value),
        label: asOptionalString(option.label),
        description: asOptionalString(option.description),
      }))
      .filter((option) => option.value),
  };
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function asString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function asOptionalString(value: unknown): string | undefined {
  const text = asString(value).trim();
  return text ? text : undefined;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}
