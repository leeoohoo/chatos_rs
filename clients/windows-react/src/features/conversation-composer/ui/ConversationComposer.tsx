import type { KeyboardEvent } from "react";
import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";
import { useConversationComposerStore } from "../application/conversationComposerStore";
import type { ConversationComposerPort } from "../ports/ConversationComposerPort";
import { ConversationRuntimeControls } from "../../conversation-runtime-settings/ui/ConversationRuntimeControls";
import type { ConversationRuntimeSettingsPort } from "../../conversation-runtime-settings/ports/ConversationRuntimeSettingsPort";
import {
  AVAILABLE_COMPOSER_CAPABILITIES,
  filterCapabilityCandidates,
  trailingCapabilityQuery,
} from "../domain/conversationComposer";

export function ConversationComposer({
  ownerUserId,
  conversationId,
  history,
  port,
  runtimeSettingsPort,
  onAccepted,
}: {
  ownerUserId: string;
  conversationId: string;
  history: ConversationHistorySnapshot;
  port: ConversationComposerPort;
  runtimeSettingsPort: ConversationRuntimeSettingsPort;
  onAccepted(): Promise<void>;
}) {
  const draft = useConversationComposerStore((state) => state.draft);
  const selectedCapability = useConversationComposerStore(
    (state) => state.explicitCapabilitySelection,
  );
  const suggestionsSuppressed = useConversationComposerStore(
    (state) => state.capabilitySuggestionsSuppressed,
  );
  const highlightedCapabilityId = useConversationComposerStore(
    (state) => state.highlightedCapabilityId,
  );
  const isSending = useConversationComposerStore((state) => state.isSending);
  const error = useConversationComposerStore((state) => state.error);
  const setDraft = useConversationComposerStore((state) => state.setDraft);
  const setSelectedCapability = useConversationComposerStore(
    (state) => state.setExplicitCapabilitySelection,
  );
  const dismissCapabilitySuggestions = useConversationComposerStore(
    (state) => state.dismissCapabilitySuggestions,
  );
  const moveCapabilityHighlight = useConversationComposerStore(
    (state) => state.moveCapabilityHighlight,
  );
  const selectCapability = useConversationComposerStore((state) => state.selectCapability);
  const send = useConversationComposerStore((state) => state.send);
  const canSend = !isSending && draft.trim().length > 0;
  const capabilityQuery = suggestionsSuppressed ? null : trailingCapabilityQuery(draft);
  const visibleCapabilities = capabilityQuery
    ? filterCapabilityCandidates(AVAILABLE_COMPOSER_CAPABILITIES, capabilityQuery).slice(0, 8)
    : [];

  const submit = () => void send(port, ownerUserId, conversationId, history, onAccepted);
  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (capabilityQuery) {
      if (event.key === "Escape") {
        event.preventDefault();
        dismissCapabilitySuggestions();
        return;
      }
      if (event.key === "ArrowUp" || event.key === "ArrowDown") {
        event.preventDefault();
        moveCapabilityHighlight(
          event.key === "ArrowDown" ? "down" : "up",
          AVAILABLE_COMPOSER_CAPABILITIES,
        );
        return;
      }
      if ((event.key === "Tab" || (event.key === "Enter" && !event.shiftKey))
          && !event.nativeEvent.isComposing) {
        event.preventDefault();
        const selected = visibleCapabilities.find(({ id }) => id === highlightedCapabilityId)
          ?? visibleCapabilities[0];
        if (selected) selectCapability(selected);
        return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      submit();
    }
  };

  return (
    <section className="conversation-composer" aria-label="消息输入">
      <ConversationRuntimeControls
        ownerUserId={ownerUserId}
        conversationId={conversationId}
        port={runtimeSettingsPort}
      />
      {selectedCapability ? (
        <div
          aria-label={`已选择能力：${selectedCapability.displayName}`}
          className="capability-chip"
        >
          <span aria-hidden="true" className="capability-chip__icon">✓</span>
          <span>{selectedCapability.displayName}</span>
          <button
            aria-label={`移除所选能力：${selectedCapability.displayName}`}
            onClick={() => setSelectedCapability(null)}
            type="button"
          >
            ×
          </button>
        </div>
      ) : null}
      {error && <p className="composer-error" role="alert">{error}</p>}
      <div className="composer-entry">
        {capabilityQuery ? (
          <div
            aria-label="能力建议"
            className="capability-suggestions"
            id="composer-capability-suggestions"
            role="listbox"
          >
            {visibleCapabilities.map((candidate) => {
              const highlighted = candidate.id === highlightedCapabilityId;
              return (
                <button
                  aria-selected={highlighted}
                  className={highlighted ? "capability-option capability-option--highlighted" : "capability-option"}
                  id={`capability-option-${candidate.id}`}
                  key={candidate.id}
                  onClick={() => selectCapability(candidate)}
                  role="option"
                  type="button"
                >
                  <span aria-hidden="true" className="capability-option__icon">✓</span>
                  <span className="capability-option__copy">
                    <strong>{candidate.selection.displayName}</strong>
                    <small>{candidate.subtitle}</small>
                  </span>
                  {highlighted ? <kbd aria-hidden="true">↩</kbd> : null}
                </button>
              );
            })}
            {visibleCapabilities.length === 0 ? (
              <p className="capability-empty">没有匹配的能力</p>
            ) : null}
          </div>
        ) : null}
        <div className="composer-input">
          <textarea
            aria-activedescendant={capabilityQuery && highlightedCapabilityId
              ? `capability-option-${highlightedCapabilityId}`
              : undefined}
            aria-controls={capabilityQuery ? "composer-capability-suggestions" : undefined}
            aria-expanded={Boolean(capabilityQuery)}
            aria-label="消息"
            aria-haspopup="listbox"
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
      </div>
    </section>
  );
}
