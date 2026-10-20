import { useCallback, useEffect } from "react";
import { ConversationComposer } from "../../conversation-composer/ui/ConversationComposer";
import { useConversationComposerStore } from "../../conversation-composer/application/conversationComposerStore";
import type { ConversationComposerPort } from "../../conversation-composer/ports/ConversationComposerPort";
import { ConversationRealtimeObserver } from "../../conversation-realtime/ui/ConversationRealtimeObserver";
import type { ConversationRealtimePort } from "../../conversation-realtime/ports/ConversationRealtimePort";
import { useConversationHistoryStore } from "../application/conversationHistoryStore";
import type { ConversationHistoryPort } from "../ports/ConversationHistoryPort";

export function ConversationHistoryView({
  ownerUserId,
  conversationId,
  port,
  composerPort,
  realtimePort,
}: {
  ownerUserId: string;
  conversationId: string;
  port: ConversationHistoryPort;
  composerPort: ConversationComposerPort;
  realtimePort: ConversationRealtimePort;
}) {
  const phase = useConversationHistoryStore((state) => state.phase);
  const load = useConversationHistoryStore((state) => state.load);
  const loadOlder = useConversationHistoryStore((state) => state.loadOlder);
  const refreshLatest = useConversationHistoryStore((state) => state.refreshLatest);
  const resetComposer = useConversationComposerStore((state) => state.reset);

  useEffect(() => {
    void load(port, ownerUserId, conversationId);
  }, [conversationId, load, ownerUserId, port]);

  useEffect(() => {
    resetComposer(ownerUserId, conversationId);
  }, [conversationId, ownerUserId, resetComposer]);

  const reconcile = useCallback(
    () => refreshLatest(port),
    [port, refreshLatest],
  );

  if (phase.kind === "idle" || phase.kind === "loading") {
    return <div className="conversation-loading"><span className="spinner" /><span>正在加载会话记录…</span></div>;
  }
  if (phase.kind === "failed") {
    return (
      <div className="conversation-error">
        <strong>会话记录加载失败</strong>
        <span>{phase.message}</span>
        <button className="primary-button" type="button" onClick={() => void load(port, ownerUserId, conversationId, true)}>重试</button>
      </div>
    );
  }

  return (
    <section className="conversation-screen">
      <ConversationRealtimeObserver
        ownerUserId={ownerUserId}
        conversationId={conversationId}
        conversationVersion={phase.snapshot.conversation.version}
        port={realtimePort}
        onAffected={reconcile}
      />
      <div className="conversation-history" aria-label="会话记录">
        {phase.snapshot.next_before_ordinal != null && (
          <button className="history-more" type="button" disabled={phase.isLoadingOlder} onClick={() => void loadOlder(port)}>
            {phase.isLoadingOlder ? "正在加载…" : "加载更早消息"}
          </button>
        )}
        {phase.olderError && <p className="history-error">{phase.olderError}</p>}
        {phase.refreshError && <p className="history-error">{phase.refreshError}</p>}
        <div className="message-list">
          {phase.snapshot.messages.map((message) => (
            <article className={`message-bubble message-bubble--${message.role}`} key={message.message_id}>
              <span>{displayText(message.content)}</span>
            </article>
          ))}
        </div>
        {phase.snapshot.messages.length === 0 && <p className="conversation-empty">还没有消息</p>}
      </div>
      <ConversationComposer
        ownerUserId={ownerUserId}
        conversationId={conversationId}
        history={phase.snapshot}
        port={composerPort}
        onAccepted={reconcile}
      />
    </section>
  );
}

function displayText(content: unknown): string {
  if (typeof content === "string") return content;
  if (content && typeof content === "object") {
    const value = content as Record<string, unknown>;
    for (const key of ["text", "content", "report", "answer", "error", "message"]) {
      if (typeof value[key] === "string" && value[key].trim()) return value[key];
    }
  }
  return "";
}
