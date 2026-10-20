import type { KeyboardEvent } from "react";
import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";
import { useConversationComposerStore } from "../application/conversationComposerStore";
import type { ConversationComposerPort } from "../ports/ConversationComposerPort";

export function ConversationComposer({
  ownerUserId,
  conversationId,
  history,
  port,
  onAccepted,
}: {
  ownerUserId: string;
  conversationId: string;
  history: ConversationHistorySnapshot;
  port: ConversationComposerPort;
  onAccepted(): Promise<void>;
}) {
  const draft = useConversationComposerStore((state) => state.draft);
  const isSending = useConversationComposerStore((state) => state.isSending);
  const error = useConversationComposerStore((state) => state.error);
  const setDraft = useConversationComposerStore((state) => state.setDraft);
  const send = useConversationComposerStore((state) => state.send);
  const canSend = !isSending && draft.trim().length > 0;

  const submit = () => void send(port, ownerUserId, conversationId, history, onAccepted);
  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      submit();
    }
  };

  return (
    <section className="conversation-composer" aria-label="消息输入">
      {error && <p className="composer-error" role="alert">{error}</p>}
      <div className="composer-input">
        <textarea
          aria-label="消息"
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="输入消息…"
          rows={1}
          value={draft}
        />
        <button
          aria-label="发送"
          className="composer-send"
          disabled={!canSend}
          onClick={submit}
          type="button"
        >
          {isSending ? <span className="mini-spinner" /> : <span aria-hidden="true">↑</span>}
        </button>
      </div>
    </section>
  );
}
