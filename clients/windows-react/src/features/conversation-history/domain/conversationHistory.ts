import type { ConversationHistoryPage } from "../../../contracts/conversationRunContract";

export type ConversationHistorySnapshot = ConversationHistoryPage;

export type ConversationHistoryPhase =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "failed"; message: string }
  | {
      kind: "ready";
      snapshot: ConversationHistorySnapshot;
      isLoadingOlder: boolean;
      olderError: string | null;
      isRefreshing: boolean;
      refreshError: string | null;
    };

export const idleConversationHistory: ConversationHistoryPhase = { kind: "idle" };

export function mergeOlderHistory(
  current: ConversationHistorySnapshot,
  older: ConversationHistorySnapshot,
): ConversationHistorySnapshot {
  return {
    conversation: older.conversation,
    turns: mergeBy(current.turns, older.turns, (turn) => turn.turn_id),
    messages: mergeBy(current.messages, older.messages, (message) => message.message_id)
      .sort((left, right) => left.ordinal - right.ordinal),
    attachments: mergeBy(
      current.attachments,
      older.attachments,
      (attachment) => attachment.attachment_id,
    ).sort((left, right) => left.ordinal - right.ordinal),
    next_before_ordinal: older.next_before_ordinal,
  };
}

export function mergeLatestHistory(
  current: ConversationHistorySnapshot,
  latest: ConversationHistorySnapshot,
): ConversationHistorySnapshot {
  return {
    conversation: latest.conversation,
    turns: mergeBy(current.turns, latest.turns, (turn) => turn.turn_id),
    messages: mergeBy(current.messages, latest.messages, (message) => message.message_id)
      .sort((left, right) => left.ordinal - right.ordinal),
    attachments: mergeBy(
      current.attachments,
      latest.attachments,
      (attachment) => attachment.attachment_id,
    ).sort((left, right) => left.ordinal - right.ordinal),
    next_before_ordinal: current.next_before_ordinal,
  };
}

function mergeBy<T>(current: T[], incoming: T[], identity: (value: T) => string): T[] {
  const values = new Map(current.map((value) => [identity(value), value]));
  for (const value of incoming) values.set(identity(value), value);
  return [...values.values()];
}
