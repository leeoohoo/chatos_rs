import { useCallback, useEffect, useState } from "react";
import { useAskUserStore } from "../../ask-user/application/askUserStore";
import {
  promptsForTurn,
  unattachedPendingPrompts,
  type AskUserPrompt,
} from "../../ask-user/domain/askUser";
import type { AskUserPort } from "../../ask-user/ports/AskUserPort";
import { AskUserPromptCard } from "../../ask-user/ui/AskUserPromptCard";
import {
  formatAttachmentSize,
  type ConversationAttachmentDraft,
} from "../../conversation-attachments/domain/conversationAttachment";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import {
  AttachmentPreview,
  ConversationComposer,
} from "../../conversation-composer/ui/ConversationComposer";
import { useConversationComposerStore } from "../../conversation-composer/application/conversationComposerStore";
import type { ConversationComposerPort } from "../../conversation-composer/ports/ConversationComposerPort";
import { ConversationRealtimeObserver } from "../../conversation-realtime/ui/ConversationRealtimeObserver";
import type { ConversationRealtimePort } from "../../conversation-realtime/ports/ConversationRealtimePort";
import type { ConversationRuntimeSettingsPort } from "../../conversation-runtime-settings/ports/ConversationRuntimeSettingsPort";
import { useRunInspectorStore } from "../../run-inspector/application/runInspectorStore";
import type { RunInspectorPort } from "../../run-inspector/ports/RunInspectorPort";
import { RunInspectorLauncher, RunInspectorSheet } from "../../run-inspector/ui/RunInspector";
import { useConversationHistoryStore } from "../application/conversationHistoryStore";
import type { ConversationHistorySnapshot } from "../domain/conversationHistory";
import type { ConversationHistoryPort } from "../ports/ConversationHistoryPort";

export function ConversationHistoryView({
  ownerUserId,
  conversationId,
  port,
  composerPort,
  attachmentPort,
  realtimePort,
  runtimeSettingsPort,
  runInspectorPort,
  askUserPort,
}: {
  ownerUserId: string;
  conversationId: string;
  port: ConversationHistoryPort;
  composerPort: ConversationComposerPort;
  attachmentPort: ConversationAttachmentPort;
  realtimePort: ConversationRealtimePort;
  runtimeSettingsPort: ConversationRuntimeSettingsPort;
  runInspectorPort: RunInspectorPort;
  askUserPort: AskUserPort;
}) {
  const phase = useConversationHistoryStore((state) => state.phase);
  const load = useConversationHistoryStore((state) => state.load);
  const loadOlder = useConversationHistoryStore((state) => state.loadOlder);
  const refreshLatest = useConversationHistoryStore((state) => state.refreshLatest);
  const resetComposer = useConversationComposerStore((state) => state.reset);
  const resetInspector = useRunInspectorStore((state) => state.reset);
  const askUserOwner = useAskUserStore((state) => state.ownerUserId);
  const askUserConversation = useAskUserStore((state) => state.conversationId);
  const askUserPrompts = useAskUserStore((state) => state.prompts);
  const askUserLoading = useAskUserStore((state) => state.isLoading);
  const askUserError = useAskUserStore((state) => state.loadError);
  const activateAskUser = useAskUserStore((state) => state.activate);
  const refreshAskUser = useAskUserStore((state) => state.refresh);

  useEffect(() => {
    void load(port, ownerUserId, conversationId);
  }, [conversationId, load, ownerUserId, port]);

  useEffect(() => {
    resetComposer(ownerUserId, conversationId, attachmentPort);
    resetInspector(ownerUserId);
  }, [attachmentPort, conversationId, ownerUserId, resetComposer, resetInspector]);

  useEffect(() => {
    void activateAskUser(askUserPort, ownerUserId, conversationId);
  }, [activateAskUser, askUserPort, conversationId, ownerUserId]);

  useEffect(() => () => {
    const composer = useConversationComposerStore.getState();
    if (composer.ownerUserId === ownerUserId && composer.conversationId === conversationId) {
      composer.reset("", "", attachmentPort);
    }
  }, [attachmentPort, conversationId, ownerUserId]);

  const reconcile = useCallback(
    async () => {
      await Promise.all([
        refreshLatest(port),
        useRunInspectorStore.getState().refresh(runInspectorPort),
        refreshAskUser(askUserPort),
      ]);
    },
    [askUserPort, port, refreshAskUser, refreshLatest, runInspectorPort],
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

  const scopedPrompts = askUserOwner === ownerUserId && askUserConversation === conversationId
    ? askUserPrompts
    : [];
  const lastMessageByTurn = new Map<string, string>();
  for (const message of phase.snapshot.messages) lastMessageByTurn.set(message.turn_id, message.message_id);
  const loadedTurnIds = new Set(phase.snapshot.turns.map((turn) => turn.turn_id));
  const unattached = unattachedPendingPrompts(scopedPrompts, loadedTurnIds);

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
            <div className={`message-row message-row--${message.role}`} key={message.message_id}>
              <article className={`message-bubble message-bubble--${message.role}`}>
                <span>{displayText(message.content)}</span>
              </article>
              <MessageAttachments
                attachmentPort={attachmentPort}
                attachments={phase.snapshot.attachments.filter(
                  (attachment) => attachment.message_id === message.message_id,
                )}
                conversationId={conversationId}
                ownerUserId={ownerUserId}
              />
              {message.role === "user" && (
                <RunInspectorLauncher
                  lookup={{
                    ownerUserId,
                    conversationId,
                    turnId: message.turn_id,
                    userMessageId: message.message_id,
                  }}
                  port={runInspectorPort}
                />
              )}
              {lastMessageByTurn.get(message.turn_id) === message.message_id && (
                <AskUserPromptList
                  prompts={promptsForTurn(scopedPrompts, message.turn_id)}
                  port={askUserPort}
                  onResolved={reconcile}
                />
              )}
            </div>
          ))}
        </div>
        {unattached.length > 0 && (
          <section className="ask-user-unattached" aria-label="等待回复的任务">
            <header><strong>等待你的回复</strong><small>对应消息尚未加载，仍可直接处理</small></header>
            <AskUserPromptList prompts={unattached} port={askUserPort} onResolved={reconcile} />
          </section>
        )}
        {askUserLoading && <span className="ask-user-loading"><span className="mini-spinner" />正在检查交互请求…</span>}
        {askUserError && <p className="history-error">交互请求刷新失败：{askUserError}</p>}
        {phase.snapshot.messages.length === 0 && <p className="conversation-empty">还没有消息</p>}
      </div>
      <ConversationComposer
        ownerUserId={ownerUserId}
        conversationId={conversationId}
        history={phase.snapshot}
        port={composerPort}
        attachmentPort={attachmentPort}
        runtimeSettingsPort={runtimeSettingsPort}
        onAccepted={reconcile}
      />
      <RunInspectorSheet port={runInspectorPort} />
    </section>
  );
}

function AskUserPromptList({
  prompts,
  port,
  onResolved,
}: {
  prompts: AskUserPrompt[];
  port: AskUserPort;
  onResolved(): Promise<void>;
}) {
  if (prompts.length === 0) return null;
  return (
    <div className="ask-user-prompt-list">
      {prompts.map((prompt) => (
        <AskUserPromptCard
          key={`${prompt.sessionId}:${prompt.id}`}
          prompt={prompt}
          port={port}
          onResolved={onResolved}
        />
      ))}
    </div>
  );
}

type PersistedAttachment = ConversationHistorySnapshot["attachments"][number];

function MessageAttachments({
  attachments,
  attachmentPort,
  ownerUserId,
  conversationId,
}: {
  attachments: PersistedAttachment[];
  attachmentPort: ConversationAttachmentPort;
  ownerUserId: string;
  conversationId: string;
}) {
  const [drafts, setDrafts] = useState<ConversationAttachmentDraft[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{
    draft: ConversationAttachmentDraft;
    authorizedLocalRef: string;
  } | null>(null);
  const references = attachments.map((attachment) => attachment.authorized_local_ref);
  const referenceKey = references.join("|");

  useEffect(() => {
    let disposed = false;
    setDrafts([]);
    setError(null);
    if (references.length === 0) return () => { disposed = true; };
    void attachmentPort.loadPersisted({
      ownerUserId,
      conversationId,
      draftGeneration: 0,
    }, references).then((loaded) => {
      if (!disposed) setDrafts(loaded);
    }).catch((reason: unknown) => {
      if (!disposed) setError(errorMessage(reason));
    });
    return () => { disposed = true; };
  // referenceKey is the stable identity for this message's immutable attachment records.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attachmentPort, conversationId, ownerUserId, referenceKey]);

  if (attachments.length === 0) return null;
  return (
    <>
      <div className="message-attachments" aria-label="消息附件">
        {drafts.map((draft) => {
          const record = attachments.find((attachment) => attachment.attachment_id === draft.id);
          if (!record) return null;
          return (
            <button
              className="message-attachment"
              key={draft.id}
              onClick={() => setPreview({
                draft,
                authorizedLocalRef: record.authorized_local_ref,
              })}
              type="button"
            >
              <span aria-hidden="true">{draft.kind === "image" ? "▧" : draft.kind === "audio" ? "♪" : "▤"}</span>
              <strong>{draft.name}</strong>
              <small>{formatAttachmentSize(draft.byteSize)}</small>
            </button>
          );
        })}
        {drafts.length === 0 && !error ? <span className="mini-spinner" aria-label="正在加载消息附件" /> : null}
        {error ? <small className="message-attachments__error">{error}</small> : null}
      </div>
      {preview ? (
        <AttachmentPreview
          attachment={preview.draft}
          attachmentPort={attachmentPort}
          authorizedLocalRef={preview.authorizedLocalRef}
          conversationId={conversationId}
          onClose={() => setPreview(null)}
          ownerUserId={ownerUserId}
        />
      ) : null}
    </>
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

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "无法读取消息附件。";
}
