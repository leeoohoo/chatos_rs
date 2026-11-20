import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import {
  formatAttachmentSize,
  mergeAttachmentDrafts,
  type AttachmentImportResult,
  type ConversationAttachmentDraft,
} from "../../conversation-attachments/domain/conversationAttachment";
import { useAgentTeamConversationStore } from "../application/agentTeamConversationStore";
import type { AgentProfileSummary, AgentTeamDetail } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { SafeMarkdownPreview } from "../../notepad/ui/SafeMarkdownPreview";
import {
  removeTrailingAgentTeamMentionQuery,
  resolveAgentTeamMentionIds,
  trailingAgentTeamMentionQuery,
  visibleAgentTeamMentionCandidates,
} from "../domain/agentTeamMention";
import { getFocusableModalElements } from "../../../shared/modalAccessibility";
import { formatTeamMessageTime } from "./AgentManagementUiPrimitives";
import {
  TeamMessageAvatar,
  TeamSystemMessage,
  teamDeliveryStatusLabel,
} from "./AgentTeamMessagePresentation";

export function AgentTeamConversationView({
  attachmentPort,
  detail,
  ownerUserId,
  port,
  profiles,
}: {
  attachmentPort?: ConversationAttachmentPort;
  detail: AgentTeamDetail;
  ownerUserId: string;
  port: AgentManagementPort;
  profiles: AgentProfileSummary[];
}) {
  const snapshot = useAgentTeamConversationStore((state) => state.snapshot);
  const isLoading = useAgentTeamConversationStore((state) => state.isLoading);
  const isLoadingOlder = useAgentTeamConversationStore((state) => state.isLoadingOlder);
  const isSending = useAgentTeamConversationStore((state) => state.isSending);
  const isCycling = useAgentTeamConversationStore((state) => state.isCycling);
  const error = useAgentTeamConversationStore((state) => state.error);
  const activate = useAgentTeamConversationStore((state) => state.activate);
  const refresh = useAgentTeamConversationStore((state) => state.refresh);
  const loadOlder = useAgentTeamConversationStore((state) => state.loadOlder);
  const send = useAgentTeamConversationStore((state) => state.send);
  const reset = useAgentTeamConversationStore((state) => state.reset);
  const [content, setContent] = useState("");
  const [mentionedAgentIds, setMentionedAgentIds] = useState<string[]>([]);
  const [attachments, setAttachments] = useState<ConversationAttachmentDraft[]>([]);
  const [attachmentError, setAttachmentError] = useState<string | null>(null);
  const [isPickingAttachments, setIsPickingAttachments] = useState(false);
  const [mentionMenuOpen, setMentionMenuOpen] = useState(false);
  const [mentionError, setMentionError] = useState<string | null>(null);
  const [mentionSuggestionsSuppressed, setMentionSuggestionsSuppressed] = useState(false);
  const [highlightedMentionIndex, setHighlightedMentionIndex] = useState(0);
  const transcriptRef = useRef<HTMLDivElement | null>(null);
  const followsLatestRef = useRef(true);
  const positionedTeamRef = useRef<string | null>(null);
  const previousLastMessageRef = useRef<string | null>(null);
  const attachmentScope = useMemo(() => ({
    ownerUserId,
    conversationId: `agent-team-compose-${detail.team.id}`,
    draftGeneration: 0,
  }), [detail.team.id, ownerUserId]);
  const attachmentsRef = useRef(attachments);
  attachmentsRef.current = attachments;
  const supportsConversation = Boolean(
    port.loadTeamConversation
    && port.queueTeamMessage
    && port.dispatchNextTeamDelivery
    && port.loadTeamMemberActivity,
  );
  const memberProfiles = useMemo(() => detail.members.map((member) => ({
    member,
    profile: profiles.find((candidate) => candidate.id === member.agentId) ?? null,
  })), [detail.members, profiles]);
  const profileNames = useMemo(
    () => new Map(profiles.map((profile) => [profile.id, profile.name])),
    [profiles],
  );
  const profilesById = useMemo(
    () => new Map(profiles.map((profile) => [profile.id, profile])),
    [profiles],
  );
  const mentionCandidates = useMemo(() => memberProfiles.flatMap(({ member, profile }) => profile ? [{
    id: member.agentId,
    name: profile.name,
    subtitle: member.role,
  }] : []), [memberProfiles]);
  const visibleMentionCandidates = useMemo(
    () => (mentionSuggestionsSuppressed ? [] : visibleAgentTeamMentionCandidates(content, mentionCandidates))
      .filter((candidate) => !mentionedAgentIds.includes(candidate.id)),
    [content, mentionCandidates, mentionedAgentIds, mentionSuggestionsSuppressed],
  );
  const resolvedMention = useMemo(
    () => resolveAgentTeamMentionIds(content, mentionedAgentIds, mentionCandidates),
    [content, mentionCandidates, mentionedAgentIds],
  );
  const visibleSnapshot = snapshot?.teamId === detail.team.id ? snapshot : null;
  const hasActiveDelivery = visibleSnapshot?.deliveries.some(
    (delivery) => delivery.status === "pending" || delivery.status === "running",
  ) ?? false;
  const canRoute = resolvedMention.ids.length > 0 || detail.team.defaultAgentId != null;
  const canSend = supportsConversation && (content.trim().length > 0 || attachments.length > 0) && canRoute
    && resolvedMention.error == null && !isSending && !isCycling;

  const acceptAttachmentResult = useCallback((result: AttachmentImportResult) => {
    if (result.scope.ownerUserId !== attachmentScope.ownerUserId
      || result.scope.conversationId !== attachmentScope.conversationId
      || result.scope.draftGeneration !== attachmentScope.draftGeneration) return;
    setAttachments((current) => mergeAttachmentDrafts(result.attachments, current));
    setAttachmentError(result.errors[0] ?? null);
  }, [attachmentScope]);

  useEffect(() => {
    if (!attachmentPort) return undefined;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void attachmentPort.activateScope(attachmentScope);
    void attachmentPort.onStaged(acceptAttachmentResult).then((listener) => {
      if (disposed) listener();
      else unlisten = listener;
    });
    return () => {
      disposed = true;
      unlisten?.();
      const ids = attachmentsRef.current.map((attachment) => attachment.id);
      if (ids.length > 0) void attachmentPort.discard(attachmentScope, ids);
      void attachmentPort.deactivateScope(attachmentScope);
    };
  }, [attachmentPort, attachmentScope]);

  const sendMessage = () => {
    if (resolvedMention.error) {
      setMentionError(resolvedMention.error);
      return;
    }
    followsLatestRef.current = true;
    void send(
      port,
      content,
      resolvedMention.ids,
      attachments.map((attachment) => attachment.id),
    ).then((succeeded) => {
    if (succeeded) {
      setContent("");
      setMentionedAgentIds([]);
      setAttachments([]);
      setAttachmentError(null);
      setMentionError(null);
    }
    });
  };

  const selectMention = (agentId: string) => {
    setMentionedAgentIds((current) => current.includes(agentId) ? current : [...current, agentId]);
    setContent((current) => removeTrailingAgentTeamMentionQuery(current));
    setMentionError(null);
    setMentionSuggestionsSuppressed(false);
    setMentionMenuOpen(false);
  };

  useEffect(() => {
    if (supportsConversation) void activate(port, ownerUserId, detail.team.id);
    else reset();
    return reset;
  }, [activate, detail.team.id, ownerUserId, port, reset, supportsConversation]);

  useEffect(() => {
    if (!supportsConversation || !hasActiveDelivery) return undefined;
    const timer = window.setInterval(() => void refresh(port), 1_000);
    return () => window.clearInterval(timer);
  }, [hasActiveDelivery, port, refresh, supportsConversation]);

  useEffect(() => {
    const memberIds = new Set(detail.members.map((member) => member.agentId));
    setMentionedAgentIds((current) => current.filter((agentId) => memberIds.has(agentId)));
  }, [detail.members]);

  useEffect(() => {
    setHighlightedMentionIndex(0);
    setMentionSuggestionsSuppressed(false);
    if (trailingAgentTeamMentionQuery(content)) setMentionMenuOpen(false);
  }, [content]);

  useLayoutEffect(() => {
    const transcript = transcriptRef.current;
    if (!transcript || !visibleSnapshot || positionedTeamRef.current === detail.team.id) return;
    positionedTeamRef.current = detail.team.id;
    transcript.scrollTop = transcript.scrollHeight;
    followsLatestRef.current = true;
  }, [detail.team.id, visibleSnapshot]);

  const lastMessageId = visibleSnapshot?.messages.at(-1)?.id ?? null;
  useLayoutEffect(() => {
    const transcript = transcriptRef.current;
    if (!transcript || !lastMessageId || previousLastMessageRef.current === lastMessageId) return;
    const hadPrevious = previousLastMessageRef.current != null;
    previousLastMessageRef.current = lastMessageId;
    if (!hadPrevious || followsLatestRef.current) transcript.scrollTop = transcript.scrollHeight;
  }, [lastMessageId]);

  if (!supportsConversation) {
    return (
      <div className="agent-team-conversation__state is-error" role="alert">
        <strong>当前版本尚未连接团队聊天</strong>
        <span>请更新本机客户端后再发送团队消息。</span>
      </div>
    );
  }

  const deliveriesByMessage = new Map<string, NonNullable<typeof visibleSnapshot>["deliveries"]>();
  for (const delivery of visibleSnapshot?.deliveries ?? []) {
    const deliveries = deliveriesByMessage.get(delivery.messageId) ?? [];
    deliveries.push(delivery);
    deliveriesByMessage.set(delivery.messageId, deliveries);
  }

  return (
    <div className="agent-team-conversation">
      <div
        aria-live="polite"
        className="agent-team-conversation__messages"
        onScroll={(event) => {
          const target = event.currentTarget;
          followsLatestRef.current = target.scrollHeight - target.scrollTop - target.clientHeight <= 32;
        }}
        ref={transcriptRef}
      >
        {visibleSnapshot?.hasOlder && (
          <button
            className="agent-team-conversation__older"
            disabled={isLoadingOlder}
            onClick={() => {
              const transcript = transcriptRef.current;
              const beforeHeight = transcript?.scrollHeight ?? 0;
              const beforeTop = transcript?.scrollTop ?? 0;
              void loadOlder(port).then((loaded) => {
                if (!loaded) return;
                window.requestAnimationFrame(() => {
                  const current = transcriptRef.current;
                  if (current) current.scrollTop = beforeTop + current.scrollHeight - beforeHeight;
                });
              });
            }}
            type="button"
          >{isLoadingOlder ? "正在加载…" : "↻ 加载更早消息"}</button>
        )}
        {isLoading && !visibleSnapshot ? (
          <div className="agent-team-conversation__state"><span className="mini-spinner" />正在读取团队消息…</div>
        ) : visibleSnapshot?.messages.length ? visibleSnapshot.messages.map((message) => {
          const senderName = message.senderKind === "human"
            ? "你"
            : message.senderKind === "system"
              ? "系统"
              : profileNames.get(message.senderId) ?? message.senderId;
          const deliveries = deliveriesByMessage.get(message.id) ?? [];
          const senderProfile = message.senderKind === "agent" ? profilesById.get(message.senderId) : null;
          if (message.senderKind === "system") {
            return <TeamSystemMessage content={message.content} key={message.id} />;
          }
          return (
            <div className={`agent-team-message-row agent-team-message-row--${message.senderKind}`} key={message.id}>
              {message.senderKind === "agent" && <TeamMessageAvatar name={senderName} profile={senderProfile} />}
              <div className="agent-team-message-column">
                <div className="agent-team-message__meta">
                  <strong>{senderName}</strong>
                  <time>{formatTeamMessageTime(message.createdAtUnixMs)}</time>
                </div>
                <div className={`agent-team-message agent-team-message--${message.senderKind}`}>
                  {message.content && <SafeMarkdownPreview markdown={message.content} />}
                  {message.attachments.length > 0 && (
                    <div className="agent-team-message__attachments" aria-label="消息附件">
                      {message.attachments.map((attachment) => (
                        <span key={attachment.id}>
                          <b>{attachment.kind === "image" ? "▧" : attachment.kind === "audio" ? "♪" : "↗"}</b>
                          {attachment.name}<small>{formatAttachmentSize(attachment.size)}</small>
                        </span>
                      ))}
                    </div>
                  )}
                  {message.mentionedAgentIds.length > 0 && (
                    <div className="agent-team-message__mentions">
                      {message.mentionedAgentIds.map((agentId) => (
                        <span key={agentId}>@{profileNames.get(agentId) ?? agentId}</span>
                      ))}
                    </div>
                  )}
                  {deliveries.length > 0 && (
                    <div className="agent-team-message__deliveries" aria-label="消息执行状态">
                      {deliveries.map((delivery) => (
                        <span className={`is-${delivery.status}`} key={delivery.id} title={delivery.lastError ?? undefined}>
                          {profileNames.get(delivery.targetAgentId) ?? delivery.targetAgentId}
                          {` · ${teamDeliveryStatusLabel(delivery.status)}`}
                          {delivery.lastError ? `：${delivery.lastError}` : ""}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
              </div>
              {message.senderKind === "human" && <TeamMessageAvatar human name="你" />}
            </div>
          );
        }) : (
          <div className="agent-team-conversation__state">
            <strong>开始团队协作</strong>
            <span>选择一个或多个 Agent，或直接交给默认 Agent。</span>
          </div>
        )}
      </div>
      {error && (
        <div className="agent-team-conversation__error" role="alert">
          <span>{error}</span>
          <button disabled={isCycling} onClick={() => void refresh(port)} type="button">重试</button>
        </div>
      )}
      <div className="agent-team-composer">
        {hasActiveDelivery && <div className="agent-team-composer__activity"><span className="mini-spinner" />Agent 正在处理群聊…</div>}
        <div aria-label="已选择的 Agent" className="agent-team-composer__mentions">
          {mentionedAgentIds.map((agentId) => (
            <button
              aria-label={`移除 @${profileNames.get(agentId) ?? agentId}`}
              key={agentId}
              onClick={() => setMentionedAgentIds((current) => current.filter((id) => id !== agentId))}
              type="button"
            >@{profileNames.get(agentId) ?? agentId}<span aria-hidden="true">×</span></button>
          ))}
          {mentionedAgentIds.length === 0 && detail.team.defaultAgentId && (
            <span>默认发送给 {profileNames.get(detail.team.defaultAgentId) ?? "默认 Agent"}</span>
          )}
        </div>
        {attachments.length > 0 && (
          <div className="agent-team-composer__attachments" aria-label="待发送附件">
            {attachments.map((attachment) => (
              <span key={attachment.id}>
                <b>{attachment.name}</b><small>{formatAttachmentSize(attachment.byteSize)}</small>
                <button
                  aria-label={`移除附件：${attachment.name}`}
                  onClick={() => {
                    if (attachmentPort) void attachmentPort.discard(attachmentScope, [attachment.id]);
                    setAttachments((current) => current.filter((item) => item.id !== attachment.id));
                  }}
                  type="button"
                >×</button>
              </span>
            ))}
          </div>
        )}
        {(attachmentError || mentionError) && (
          <div className="agent-team-conversation__error" role="alert">{attachmentError ?? mentionError}</div>
        )}
        <div className="agent-team-composer__surface">
          <div className="agent-team-composer__tools">
            <div
              className="agent-team-mention-picker"
              onBlur={(event) => {
                if (!event.currentTarget.contains(event.relatedTarget)) setMentionMenuOpen(false);
              }}
              onKeyDown={(event) => {
                if (event.key === "Escape") setMentionMenuOpen(false);
              }}
            >
              <button
                aria-expanded={mentionMenuOpen}
                aria-label="选择要 @ 的 Agent"
                disabled={mentionCandidates.length === 0}
                onClick={() => setMentionMenuOpen((open) => !open)}
                type="button"
              >@</button>
              {mentionMenuOpen && (
                <div className="agent-team-mention-picker__menu" role="menu">
                  {mentionCandidates.map((candidate) => {
                    const selected = mentionedAgentIds.includes(candidate.id);
                    return (
                      <button
                        key={candidate.id}
                        aria-checked={selected}
                        onClick={() => {
                          setMentionedAgentIds((current) => selected
                            ? current.filter((id) => id !== candidate.id)
                            : [...current, candidate.id]);
                          setMentionError(null);
                        }}
                        role="menuitemcheckbox"
                        type="button"
                      ><span>{selected ? "✓" : "○"}</span><strong>{candidate.name}</strong><small>{candidate.subtitle}</small></button>
                    );
                  })}
                </div>
              )}
            </div>
            {attachmentPort && (
            <button
              aria-label="添加团队附件"
              disabled={isPickingAttachments || isSending}
              onClick={() => {
                setIsPickingAttachments(true);
                setAttachmentError(null);
                void attachmentPort.pickFiles(attachmentScope)
                  .then(acceptAttachmentResult)
                  .catch((failure: unknown) => setAttachmentError(
                    failure instanceof Error ? failure.message : "附件暂时无法添加。",
                  ))
                  .finally(() => setIsPickingAttachments(false));
              }}
              type="button"
            >📎</button>
            )}
          </div>
          <div className="agent-team-composer__input">
            {visibleMentionCandidates.length > 0 && (
              <div className="agent-team-mention-suggestions" role="listbox">
                {visibleMentionCandidates.map((candidate, index) => (
                  <button
                    aria-selected={index === highlightedMentionIndex}
                    key={candidate.id}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => selectMention(candidate.id)}
                    role="option"
                    type="button"
                  ><TeamMessageAvatar compact name={candidate.name} profile={profilesById.get(candidate.id)} /><span><strong>@{candidate.name}</strong><small>{candidate.subtitle}</small></span></button>
                ))}
              </div>
            )}
            <textarea
              aria-label="团队消息"
              disabled={isSending}
              maxLength={1_000_000}
              onChange={(event) => { setContent(event.target.value); setMentionError(null); }}
              onKeyDown={(event) => {
                if (visibleMentionCandidates.length > 0 && ["ArrowDown", "ArrowUp"].includes(event.key)) {
                  event.preventDefault();
                  setHighlightedMentionIndex((current) => (
                    current + (event.key === "ArrowDown" ? 1 : -1) + visibleMentionCandidates.length
                  ) % visibleMentionCandidates.length);
                  return;
                }
                if (event.key === "Escape" && visibleMentionCandidates.length > 0) {
                  event.preventDefault();
                  setMentionSuggestionsSuppressed(true);
                  return;
                }
                if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
                  event.preventDefault();
                  if (visibleMentionCandidates.length > 0) selectMention(
                    visibleMentionCandidates[Math.min(highlightedMentionIndex, visibleMentionCandidates.length - 1)]!.id,
                  );
                  else if (canSend) sendMessage();
                }
              }}
              placeholder={canRoute
                ? "输入消息；不选择 @ 时交给默认 Agent，也可添加图片、文档和长文本…"
                : "请先选择一个 Agent"}
              rows={2}
              value={content}
            />
            <button
              aria-label="发送团队消息"
              className="primary-button"
              disabled={!canSend}
              onClick={sendMessage}
              type="button"
            >{isSending ? <span className="mini-spinner" /> : "↑"}</button>
          </div>
        </div>
        {!canRoute && <small>当前团队没有默认 Agent，请先选择一个接收者。</small>}
      </div>
    </div>
  );
}
