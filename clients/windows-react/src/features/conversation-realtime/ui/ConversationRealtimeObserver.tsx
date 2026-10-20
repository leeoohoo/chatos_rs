import { useEffect } from "react";
import { useConversationRealtimeStore } from "../application/conversationRealtimeStore";
import type { ConversationRealtimePort } from "../ports/ConversationRealtimePort";

const POLL_INTERVAL_MILLISECONDS = 850;
const RETRY_INTERVAL_MILLISECONDS = 1_500;

export function ConversationRealtimeObserver({
  ownerUserId,
  conversationId,
  conversationVersion,
  port,
  onAffected,
}: {
  ownerUserId: string;
  conversationId: string;
  conversationVersion: number;
  port: ConversationRealtimePort;
  onAffected(): Promise<void>;
}) {
  const activate = useConversationRealtimeStore((state) => state.activate);
  const poll = useConversationRealtimeStore((state) => state.poll);

  useEffect(() => {
    activate(ownerUserId, conversationId, conversationVersion);
  }, [activate, conversationId, conversationVersion, ownerUserId]);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const run = async () => {
      await poll(port, onAffected);
      if (cancelled) return;
      const failed = useConversationRealtimeStore.getState().snapshot.error != null;
      timer = setTimeout(run, failed ? RETRY_INTERVAL_MILLISECONDS : POLL_INTERVAL_MILLISECONDS);
    };
    void run();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [conversationId, onAffected, ownerUserId, poll, port]);

  return null;
}
