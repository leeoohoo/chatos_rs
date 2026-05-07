import type {
  TaskReviewDraft,
  UiPromptChoice,
  UiPromptField,
  UiPromptKind,
} from '../store/types';

type AnyRecord = Record<string, unknown>;

const asRecord = (value: unknown): AnyRecord | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as AnyRecord
    : null
);

export const normalizeTaskPriority = (value: unknown): TaskReviewDraft['priority'] => {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'high') return 'high';
  if (normalized === 'low') return 'low';
  return 'medium';
};

export const normalizeTaskStatus = (value: unknown): TaskReviewDraft['status'] => {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'doing') return 'doing';
  if (normalized === 'blocked') return 'blocked';
  if (normalized === 'done') return 'done';
  return 'todo';
};

export const parseTaskTags = (value: unknown): string[] => {
  const source = Array.isArray(value)
    ? value
    : typeof value === 'string'
      ? value.split(',')
      : [];

  const seen = new Set<string>();
  const tags: string[] = [];
  source.forEach((item) => {
    const tag = String(item ?? '').trim();
    if (!tag || seen.has(tag)) {
      return;
    }
    seen.add(tag);
    tags.push(tag);
  });
  return tags;
};

export const normalizeUiPromptKind = (value: unknown): UiPromptKind => {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'choice') return 'choice';
  if (normalized === 'mixed') return 'mixed';
  return 'kv';
};

export const normalizeUiPromptFields = (value: unknown): UiPromptField[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map((item) => {
      const key = String(item?.key ?? '').trim();
      if (!key) {
        return null;
      }
      return {
        key,
        label: typeof item?.label === 'string' ? item.label : '',
        description: typeof item?.description === 'string' ? item.description : '',
        placeholder: typeof item?.placeholder === 'string' ? item.placeholder : '',
        default: typeof item?.default === 'string' ? item.default : '',
        required: item?.required === true,
        multiline: item?.multiline === true,
        secret: item?.secret === true,
      } satisfies UiPromptField;
    })
    .filter(Boolean) as UiPromptField[];
};

export const normalizeUiPromptChoice = (value: unknown): UiPromptChoice | undefined => {
  const source = asRecord(value);
  if (!source) {
    return undefined;
  }

  const optionsRaw = Array.isArray(source.options) ? source.options : [];
  const options = optionsRaw
    .map((item) => {
      const option = asRecord(item) || {};
      const optionValue = String(option.value ?? '').trim();
      if (!optionValue) {
        return null;
      }
      return {
        value: optionValue,
        label: typeof option.label === 'string' ? option.label : '',
        description: typeof option.description === 'string' ? option.description : '',
      };
    })
    .filter(Boolean) as UiPromptChoice['options'];

  if (options.length === 0) {
    return undefined;
  }

  const multiple = source.multiple === true;
  const minRaw = Number(source.min_selections ?? (multiple ? 0 : 0));
  const maxRaw = Number(source.max_selections ?? (multiple ? options.length : 1));
  const minSelections = Number.isFinite(minRaw) ? Math.max(0, Math.floor(minRaw)) : 0;
  const maxSelections = Number.isFinite(maxRaw)
    ? Math.max(0, Math.floor(maxRaw))
    : (multiple ? options.length : 1);

  return {
    multiple,
    options,
    default: source.default as UiPromptChoice['default'],
    min_selections: Math.min(minSelections, maxSelections),
    max_selections: maxSelections,
  };
};
