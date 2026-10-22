import { z } from "zod";

export const globalUtilityActions = [
  "screenshot",
  "screenRecording",
  "clipboardHistory",
  "quickSearch",
] as const;

export type GlobalUtilityAction = typeof globalUtilityActions[number];

export const shortcutModifierSchema = z.enum(["control", "alt", "shift", "super"]);
export type ShortcutModifier = z.infer<typeof shortcutModifierSchema>;

export const globalShortcutSchema = z.object({
  key: z.string().min(1).max(16),
  modifiers: z.array(shortcutModifierSchema).min(1).max(4),
}).strict();

export type GlobalShortcut = z.infer<typeof globalShortcutSchema>;

const registrationSchema = z.object({
  state: z.enum(["registered", "conflict", "unsupported", "disabled"]),
  requested: globalShortcutSchema,
  active: globalShortcutSchema.nullable(),
  usesFallback: z.boolean(),
}).strict();

const shortcutMapSchema = z.object({
  screenshot: globalShortcutSchema,
  screenRecording: globalShortcutSchema,
  clipboardHistory: globalShortcutSchema,
  quickSearch: globalShortcutSchema,
}).strict();

const registrationMapSchema = z.object({
  screenshot: registrationSchema,
  screenRecording: registrationSchema,
  clipboardHistory: registrationSchema,
  quickSearch: registrationSchema,
}).strict();

export const globalUtilityPreferencesSchema = z.object({
  isEnabled: z.boolean(),
  hasAcknowledgedShortcutConflicts: z.boolean(),
  screenshotEnabled: z.boolean(),
  screenRecordingEnabled: z.boolean(),
  clipboardHistoryEnabled: z.boolean(),
  quickSearchEnabled: z.boolean(),
  hotKeys: shortcutMapSchema,
  registrations: registrationMapSchema,
  revision: z.number().int().positive(),
}).strict();

export type GlobalUtilityPreferences = z.infer<typeof globalUtilityPreferencesSchema>;

export const defaultGlobalHotKeys: GlobalUtilityPreferences["hotKeys"] = {
  screenshot: { key: "A", modifiers: ["control"] },
  screenRecording: { key: "Q", modifiers: ["control"] },
  clipboardHistory: { key: "E", modifiers: ["control"] },
  quickSearch: { key: "Space", modifiers: ["control"] },
};

export function sameGlobalUtilityPreferences(
  left: GlobalUtilityPreferences,
  right: GlobalUtilityPreferences,
) {
  return left.isEnabled === right.isEnabled
    && left.hasAcknowledgedShortcutConflicts === right.hasAcknowledgedShortcutConflicts
    && left.screenshotEnabled === right.screenshotEnabled
    && left.screenRecordingEnabled === right.screenRecordingEnabled
    && left.clipboardHistoryEnabled === right.clipboardHistoryEnabled
    && left.quickSearchEnabled === right.quickSearchEnabled
    && globalUtilityActions.every((action) => sameShortcut(left.hotKeys[action], right.hotKeys[action]));
}

export function formatShortcut(shortcut: GlobalShortcut) {
  const modifierLabels: Record<ShortcutModifier, string> = {
    control: "Ctrl",
    alt: "Alt",
    shift: "Shift",
    super: "⊞",
  };
  return [...shortcut.modifiers.map((modifier) => modifierLabels[modifier]), shortcut.key].join(" + ");
}

function sameShortcut(left: GlobalShortcut, right: GlobalShortcut) {
  return left.key === right.key
    && left.modifiers.length === right.modifiers.length
    && left.modifiers.every((modifier, index) => modifier === right.modifiers[index]);
}
