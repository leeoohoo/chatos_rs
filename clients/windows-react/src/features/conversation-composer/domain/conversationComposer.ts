import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";

export interface ExplicitCapabilitySelection {
  kind: "requirement_survey" | "plugin";
  displayName: string;
  pluginKey?: string;
  componentKey?: string;
}

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
