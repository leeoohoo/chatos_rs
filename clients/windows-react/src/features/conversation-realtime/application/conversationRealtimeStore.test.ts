import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConversationRealtimePort } from "../ports/ConversationRealtimePort";
import { useConversationRealtimeStore } from "./conversationRealtimeStore";

describe("conversation realtime store", () => {
  beforeEach(() => {
    useConversationRealtimeStore.getState().reset();
    useConversationRealtimeStore.getState().activate("owner-1", "conversation-1", 3);
  });

  it("advances the durable cursor and reconciles an affected conversation", async () => {
    const port: ConversationRealtimePort = {
      poll: vi.fn().mockResolvedValue({
        nextCursor: 18,
        conversationVersion: 4,
        affected: true,
      }),
    };
    const reconcile = vi.fn().mockResolvedValue(undefined);

    await useConversationRealtimeStore.getState().poll(port, reconcile);

    expect(port.poll).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      conversationId: "conversation-1",
      afterCursor: null,
      observedConversationVersion: 3,
    });
    expect(reconcile).toHaveBeenCalledOnce();
    expect(useConversationRealtimeStore.getState().snapshot).toEqual({
      cursor: 18,
      conversationVersion: 4,
      error: null,
    });
  });

  it("rejects a late poll after activation moves to another conversation", async () => {
    let resolve!: (value: { nextCursor: number; conversationVersion: number; affected: boolean }) => void;
    const port: ConversationRealtimePort = {
      poll: () => new Promise((completion) => { resolve = completion; }),
    };
    const pending = useConversationRealtimeStore.getState().poll(port, async () => undefined);
    useConversationRealtimeStore.getState().activate("owner-1", "conversation-2", 1);
    resolve({ nextCursor: 40, conversationVersion: 9, affected: true });
    await pending;

    expect(useConversationRealtimeStore.getState()).toMatchObject({
      conversationId: "conversation-2",
      requestId: 0,
      snapshot: { cursor: null, conversationVersion: 1 },
    });
  });
});
