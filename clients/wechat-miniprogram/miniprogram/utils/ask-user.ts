import type {
  AskUserChoice,
  AskUserChoiceOption,
  AskUserField,
  AskUserPromptRecord,
} from '../models/api'

type UnknownMap = Record<string, unknown>

export type AskUserPromptView = {
  id: string
  title: string
  message: string
  allowCancel: boolean
  fields: AskUserField[]
  choice?: AskUserChoice
  values: Record<string, string>
  selection: string[]
  submitting: boolean
  valid: boolean
}

function object(value: unknown): UnknownMap {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as UnknownMap) : {}
}

function text(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined
}

function number(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined
}

function parseField(value: unknown, index: number): AskUserField | undefined {
  const raw = object(value)
  const label = (text(raw.label) ?? '').trim()
  const key = ((text(raw.key) ?? text(raw.name) ?? text(raw.id) ?? label) || `field_${index + 1}`).trim()
  if (!key) return undefined
  return {
    key,
    label: label || key,
    description: text(raw.description),
    placeholder: text(raw.placeholder),
    defaultValue: text(raw.default_value) ?? text(raw.default) ?? '',
    required: raw.required === true,
    multiline: raw.multiline === true,
    secret: raw.secret === true,
  }
}

function parseChoice(value: unknown): AskUserChoice | undefined {
  const raw = object(value)
  const options = (Array.isArray(raw.options) ? raw.options : [])
    .map((item): AskUserChoiceOption | undefined => {
      const option = object(item)
      const value = text(option.value)
      if (value === undefined) return undefined
      return {
        value,
        label: text(option.label) ?? value,
        description: text(option.description),
      }
    })
    .filter((option): option is AskUserChoiceOption => Boolean(option))
  if (!options.length) return undefined
  const rawDefault = raw.default
  const defaults = Array.isArray(rawDefault)
    ? rawDefault.filter((item): item is string => typeof item === 'string')
    : typeof rawDefault === 'string'
      ? [rawDefault]
      : []
  const multiple = raw.multiple === true
  const minimum = Math.max(0, number(raw.min_selections) ?? 0)
  return {
    multiple,
    options,
    defaults,
    minimum,
    maximum: Math.max(minimum, number(raw.max_selections) ?? (multiple ? options.length : 1)),
  }
}

export function promptView(
  record: AskUserPromptRecord,
  draftValues: Record<string, string> = {},
  draftSelection?: string[],
  submitting = false,
): AskUserPromptView {
  const stored = object(record.prompt)
  const payload = object(stored.payload)
  const fields = (Array.isArray(payload.fields) ? payload.fields : [])
    .map(parseField)
    .filter((field): field is AskUserField => Boolean(field))
  const choice = parseChoice(payload.choice)
  const values = Object.fromEntries(
    fields.map((field) => [field.key, draftValues[field.key] ?? field.defaultValue]),
  )
  const selection = draftSelection ?? choice?.defaults ?? []
  const displayedChoice = choice ? {
    ...choice,
    options: choice.options.map((option) => ({
      ...option,
      selected: selection.includes(option.value),
    })),
  } : undefined
  const requiredFieldsValid = fields.every(
    (field) => !field.required || Boolean(values[field.key]?.trim()),
  )
  const selectionValid = !choice || (
    selection.length >= choice.minimum && selection.length <= choice.maximum
  )
  return {
    id: record.id,
    title: text(stored.title)?.trim() || '需要你的输入',
    message: text(stored.message) ?? '',
    allowCancel: stored.allow_cancel !== false,
    fields,
    choice: displayedChoice,
    values,
    selection,
    submitting,
    valid: requiredFieldsValid && selectionValid,
  }
}
