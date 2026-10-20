import { useEffect } from "react";
import { effectiveThinkingLevel, selectedModel } from "../domain/conversationRuntimeSettings";
import { useConversationRuntimeSettingsStore } from "../application/conversationRuntimeSettingsStore";
import type { ConversationRuntimeSettingsPort } from "../ports/ConversationRuntimeSettingsPort";

export function ConversationRuntimeControls({
  ownerUserId,
  conversationId,
  port,
}: {
  ownerUserId: string;
  conversationId: string;
  port: ConversationRuntimeSettingsPort;
}) {
  const snapshot = useConversationRuntimeSettingsStore((state) => state.snapshot);
  const isLoading = useConversationRuntimeSettingsStore((state) => state.isLoading);
  const isUpdating = useConversationRuntimeSettingsStore((state) => state.isUpdating);
  const error = useConversationRuntimeSettingsStore((state) => state.error);
  const load = useConversationRuntimeSettingsStore((state) => state.load);
  const reset = useConversationRuntimeSettingsStore((state) => state.reset);
  const selectModel = useConversationRuntimeSettingsStore((state) => state.selectModel);
  const selectReasoningLevel = useConversationRuntimeSettingsStore(
    (state) => state.selectReasoningLevel,
  );
  const selectRemoteConnection = useConversationRuntimeSettingsStore(
    (state) => state.selectRemoteConnection,
  );

  useEffect(() => {
    void load(port, ownerUserId, conversationId);
    return reset;
  }, [conversationId, load, ownerUserId, port, reset]);

  if (!snapshot) {
    return (
      <div className="runtime-controls runtime-controls--loading" aria-live="polite">
        {isLoading ? <span className="mini-spinner" aria-hidden="true" /> : null}
        <span>{error ?? "正在读取运行配置…"}</span>
      </div>
    );
  }

  const model = selectedModel(snapshot);
  const disabled = isLoading || isUpdating;
  return (
    <>
      {error ? <p className="composer-error" role="alert">{error}</p> : null}
      <div className="runtime-controls" aria-label="会话运行配置">
        <label>
          <span className="sr-only">模型</span>
          <select
            aria-label="模型"
            disabled={disabled}
            onChange={(event) => void selectModel(port, event.target.value)}
            value={snapshot.settings.selectedModelId}
          >
            {snapshot.models.map((option) => (
              <option key={option.id} value={option.id}>{option.displayName}</option>
            ))}
          </select>
        </label>
        <label>
          <span className="sr-only">远端连接</span>
          <select
            aria-label="远端连接"
            disabled={disabled}
            onChange={(event) => void selectRemoteConnection(
              port,
              event.target.value || null,
            )}
            value={snapshot.settings.remoteConnectionId ?? ""}
          >
            <option value="">未绑定远端</option>
            {snapshot.remoteConnections.map((connection) => (
              <option key={connection.id} value={connection.id}>{connection.name}</option>
            ))}
          </select>
        </label>
        {model?.supportsReasoning && model.thinkingLevels.length > 0 ? (
          <label>
            <span className="sr-only">推理等级</span>
            <select
              aria-label="推理等级"
              className="runtime-reasoning"
              disabled={disabled}
              onChange={(event) => void selectReasoningLevel(port, event.target.value)}
              value={effectiveThinkingLevel(snapshot)}
            >
              {model.thinkingLevels.map((level) => (
                <option key={level} value={level}>{levelName(level)}</option>
              ))}
            </select>
          </label>
        ) : null}
        {isUpdating ? <span className="mini-spinner" aria-label="正在更新运行配置" /> : null}
      </div>
    </>
  );
}

function levelName(level: string) {
  return ({
    none: "推理：关",
    auto: "推理：自动",
    minimal: "推理：极低",
    low: "推理：低",
    medium: "推理：中",
    high: "推理：高",
    xhigh: "推理：极高",
    max: "推理：最大",
  } as Record<string, string>)[level] ?? `推理：${level}`;
}
