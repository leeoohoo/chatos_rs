import { z } from "zod";

const fieldSchema = z.object({
  key: z.string().min(1),
  label: z.string().min(1),
  description: z.string().nullable().optional(),
  placeholder: z.string().nullable().optional(),
  defaultValue: z.string(),
  isRequired: z.boolean(),
  isMultiline: z.boolean(),
  isSecret: z.boolean(),
});

const choiceSchema = z.object({
  allowsMultiple: z.boolean(),
  options: z.array(z.object({
    value: z.string().min(1),
    label: z.string().min(1),
    description: z.string().nullable().optional(),
  })).min(1),
  defaultSelection: z.array(z.string()),
  minimumSelectionCount: z.number().int().nonnegative(),
  maximumSelectionCount: z.number().int().nonnegative(),
});

export const askUserPromptSchema = z.object({
  id: z.string().startsWith("local-ask:"),
  sessionId: z.string().min(1),
  turnId: z.string().min(1),
  toolCallId: z.string().min(1).nullable().optional(),
  kind: z.string().min(1),
  status: z.enum(["pending", "ok", "canceled", "timeout", "failed"]),
  title: z.string(),
  message: z.string(),
  allowsCancel: z.boolean(),
  timeoutMilliseconds: z.number().int().nonnegative().nullable().optional(),
  fields: z.array(fieldSchema),
  choice: choiceSchema.nullable().optional(),
  createdAtUnixMs: z.number().int().nonnegative(),
  updatedAtUnixMs: z.number().int().nonnegative(),
});

export const askUserPromptsSchema = z.array(askUserPromptSchema);

export type AskUserPrompt = z.infer<typeof askUserPromptSchema>;
export type AskUserChoice = NonNullable<AskUserPrompt["choice"]>;
export type AskUserFormState = { values: Record<string, string>; selection: Set<string> };
export type AskUserSubmission = {
  values: Record<string, string>;
  selection: string | string[] | null;
};

export function createAskUserForm(prompt: AskUserPrompt): AskUserFormState {
  return {
    values: Object.fromEntries(prompt.fields.map((field) => [field.key, field.defaultValue])),
    selection: new Set(prompt.choice?.defaultSelection ?? []),
  };
}
export function isAskUserFormValid(prompt: AskUserPrompt, form: AskUserFormState) {
  if (prompt.fields.some(
    (field) => field.isRequired && !(form.values[field.key] ?? "").trim(),
  )) return false;
  if (prompt.choice) {
    const count = form.selection.size;
    if (count < prompt.choice.minimumSelectionCount
      || count > prompt.choice.maximumSelectionCount) return false;
  }
  return true;
}

export function askUserSubmission(
  prompt: AskUserPrompt,
  form: AskUserFormState,
): AskUserSubmission {
  const ordered = prompt.choice?.options
    .map((option) => option.value)
    .filter((value) => form.selection.has(value)) ?? [];
  return {
    values: { ...form.values },
    selection: prompt.choice
      ? (prompt.choice.allowsMultiple ? ordered : (ordered[0] ?? ""))
      : null,
  };
}

export function toggleAskUserSelection(
  form: AskUserFormState,
  choice: AskUserChoice,
  value: string,
): AskUserFormState {
  if (!choice.allowsMultiple) return { ...form, selection: new Set([value]) };
  const selection = new Set(form.selection);
  if (selection.has(value)) selection.delete(value);
  else if (selection.size < choice.maximumSelectionCount) selection.add(value);
  return { ...form, selection };
}

export function clearAskUserSecrets(
  form: AskUserFormState,
  prompt: AskUserPrompt,
): AskUserFormState {
  const values = { ...form.values };
  for (const field of prompt.fields) if (field.isSecret) values[field.key] = "";
  return { ...form, values };
}

export function promptsForTurn(prompts: AskUserPrompt[], turnId: string) {
  return prompts.filter((prompt) => prompt.turnId === turnId).sort(comparePrompts);
}

export function unattachedPendingPrompts(prompts: AskUserPrompt[], loadedTurnIds: Set<string>) {
  return prompts
    .filter((prompt) => prompt.status === "pending" && !loadedTurnIds.has(prompt.turnId))
    .sort(comparePrompts);
}

export function upsertAskUserPrompt(prompts: AskUserPrompt[], prompt: AskUserPrompt) {
  const next = prompts.filter((candidate) => candidate.id !== prompt.id);
  next.push(prompt);
  return next.sort(comparePrompts);
}

function comparePrompts(left: AskUserPrompt, right: AskUserPrompt) {
  return left.createdAtUnixMs - right.createdAtUnixMs || left.id.localeCompare(right.id);
}
