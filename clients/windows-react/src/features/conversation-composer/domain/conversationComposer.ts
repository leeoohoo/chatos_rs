import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";

export interface ExplicitCapabilitySelection {
  kind: "requirement_survey" | "plugin";
  displayName: string;
  pluginKey?: string;
  componentKey?: string;
}

export interface ComposerCapabilityCandidate {
  id: string;
  selection: ExplicitCapabilitySelection;
  subtitle: string;
}

export interface ComposerCapabilityQuery {
  start: number;
  end: number;
  value: string;
}

export const REQUIREMENT_SURVEY_CAPABILITY: ComposerCapabilityCandidate = {
  id: "requirement_survey",
  selection: {
    kind: "requirement_survey",
    displayName: "需求调研",
  },
  subtitle: "先澄清目标、范围、偏好和验收标准",
};

// Plugins deliberately stay out of this catalog until the Windows platform
// worker can execute the same capability bindings that macOS exposes.
export const AVAILABLE_COMPOSER_CAPABILITIES: readonly ComposerCapabilityCandidate[] = [
  REQUIREMENT_SURVEY_CAPABILITY,
];

export interface ConversationDraftSubmission {
  content: string;
  explicitCapabilitySelection: ExplicitCapabilitySelection | null;
}

export type ConversationSendPlan =
  | { kind: "ignore" }
  | { kind: "blocked"; message: string }
  | { kind: "guidance"; submission: ConversationDraftSubmission; turnId: string }
  | { kind: "newTurn"; submission: ConversationDraftSubmission };

export interface ConversationSendAck {
  deliveryKind: "guidance" | "newTurn";
  turnId: string;
  userMessageId: string;
}

export function planConversationSend(
  draft: string,
  explicitCapabilitySelection: ExplicitCapabilitySelection | null,
  history: ConversationHistorySnapshot,
  isSending: boolean,
): ConversationSendPlan {
  const content = draft.trim();
  if (!content || isSending) return { kind: "ignore" };
  const submission = { content, explicitCapabilitySelection };
  const activeTurn = [...history.turns].reverse().find((turn) => turn.status === "running");
  if (!activeTurn) return { kind: "newTurn", submission };
  if (explicitCapabilitySelection) {
    return {
      kind: "blocked",
      message: "当前轮次仍在执行。请等待结束或停止后，再用所选能力发起新任务。",
    };
  }
  return { kind: "guidance", submission, turnId: activeTurn.turn_id };
}

export function recoverComposerDraft(currentDraft: string, failedDraft: string): string {
  return currentDraft.length === 0 ? failedDraft : currentDraft;
}

export function trailingCapabilityQuery(text: string): ComposerCapabilityQuery | null {
  const start = text.lastIndexOf("/");
  if (start < 0) return null;
  if (start > 0 && !isWhitespace(text[start - 1] ?? "")) return null;
  const value = text.slice(start + 1);
  if (Array.from(value).length > 80 || Array.from(value).some(isWhitespace)) return null;
  return { start, end: text.length, value };
}

export function removeTrailingCapabilityQuery(
  text: string,
  query: ComposerCapabilityQuery,
): string {
  return text.slice(0, query.start) + text.slice(query.end);
}

export function filterCapabilityCandidates(
  candidates: readonly ComposerCapabilityCandidate[],
  query: ComposerCapabilityQuery,
): ComposerCapabilityCandidate[] {
  const term = query.value.trim().toLocaleLowerCase();
  if (!term) return [...candidates];
  return candidates.filter(({ selection, subtitle }) => [
    selection.displayName,
    subtitle,
    selection.pluginKey ?? "",
  ].some((value) => value.toLocaleLowerCase().includes(term)));
}

function isWhitespace(value: string): boolean {
  return /\s/u.test(value);
}
