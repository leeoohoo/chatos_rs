import { useEffect, useMemo, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import {
  fetchTurnProcessMessages,
  resolveTurnProcessKeyForUserMessage,
} from '../../lib/store/helpers/messages';
import {
  getMessageConversationTurnId,
  getMessageHistoryFinalForTurnId,
  getMessageHistoryFinalForUserMessageId,
  getMessageHistoryProcessTurnId,
  getMessageHistoryProcessUserMessageId,
} from '../messageItem/messageReaders';
import type { Message } from '../../types';
import {
  buildTurnProcessTimeline,
  type TurnProcessTimelineItem,
} from './buildTurnProcessTimeline';
import { useI18n } from '../../i18n/I18nProvider';

const normalizeId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const mergeUniqueMessages = (base: Message[], extra: Message[]): Message[] => {
  const seen = new Set<string>();
  const merged: Message[] = [];

  [...base, ...extra].forEach((message) => {
    const id = normalizeId(message?.id);
    if (!id || seen.has(id)) {
      return;
    }
    seen.add(id);
    merged.push(message);
  });

  return merged;
};

export const useTurnProcessViewerModel = ({
  open,
  sessionId,
  userMessageId,
  turnId,
  messages,
  cachedProcessMessages,
  apiClient,
}: {
  open: boolean;
  sessionId: string | null;
  userMessageId: string | null;
  turnId?: string | null;
  messages: Message[];
  cachedProcessMessages?: Record<string, Message[]> | null;
  apiClient: ApiClient;
}) => {
  const { t } = useI18n();
  const normalizedUserMessageId = normalizeId(userMessageId);
  const normalizedTurnId = normalizeId(turnId);

  const userMessage = useMemo(() => {
    if (!open || !normalizedUserMessageId) {
      return null;
    }
    return messages.find((message) => (
      message.role === 'user' && message.id === normalizedUserMessageId
    )) || null;
  }, [messages, normalizedUserMessageId, open]);

  const resolvedTurnId = useMemo(() => {
    if (normalizedTurnId) {
      return normalizedTurnId;
    }
    if (!userMessage) {
      return '';
    }
    return normalizeId(
      getMessageConversationTurnId(userMessage)
      || getMessageHistoryProcessTurnId(userMessage),
    );
  }, [normalizedTurnId, userMessage]);

  const localProcessMessages = useMemo(() => {
    if (!open || !normalizedUserMessageId) {
      return [] as Message[];
    }

    return messages.filter((message) => {
      const processUserMessageId = getMessageHistoryProcessUserMessageId(message);
      const processTurnId = getMessageHistoryProcessTurnId(message);
      return (
        processUserMessageId === normalizedUserMessageId
        || (resolvedTurnId && processTurnId === resolvedTurnId)
      );
    });
  }, [messages, normalizedUserMessageId, open, resolvedTurnId]);

  const cachedProcessMessagesForTurn = useMemo(() => {
    if (!open || !normalizedUserMessageId || !cachedProcessMessages) {
      return [] as Message[];
    }

    const processKey = resolveTurnProcessKeyForUserMessage(messages, normalizedUserMessageId);
    const cached = (
      (processKey ? cachedProcessMessages[processKey] : undefined)
      || cachedProcessMessages[normalizedUserMessageId]
      || []
    );

    return Array.isArray(cached) ? cached : [];
  }, [cachedProcessMessages, messages, normalizedUserMessageId, open]);

  const streamingFinalAssistant = useMemo(() => {
    if (!open || !normalizedUserMessageId) {
      return null;
    }
    return messages.find((message) => (
      message.role === 'assistant'
      && message.status === 'streaming'
      && (
        getMessageHistoryFinalForUserMessageId(message) === normalizedUserMessageId
        || (resolvedTurnId && (
          getMessageHistoryFinalForTurnId(message) === resolvedTurnId
          || getMessageConversationTurnId(message) === resolvedTurnId
        ))
      )
    )) || null;
  }, [messages, normalizedUserMessageId, open, resolvedTurnId]);

  const finalAssistantMessage = useMemo(() => {
    if (!open || !normalizedUserMessageId) {
      return null;
    }
    return messages.find((message) => (
      message.role === 'assistant'
      && (
        getMessageHistoryFinalForUserMessageId(message) === normalizedUserMessageId
        || (resolvedTurnId && (
          getMessageHistoryFinalForTurnId(message) === resolvedTurnId
          || getMessageConversationTurnId(message) === resolvedTurnId
        ))
      )
    )) || null;
  }, [messages, normalizedUserMessageId, open, resolvedTurnId]);

  const [fetchedProcessMessages, setFetchedProcessMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !sessionId || !normalizedUserMessageId) {
      setFetchedProcessMessages([]);
      setLoading(false);
      setError(null);
      return;
    }

    let active = true;
    setLoading(true);
    setError(null);

    void fetchTurnProcessMessages(
      apiClient,
      sessionId,
      normalizedUserMessageId,
      { turnId: resolvedTurnId || undefined },
    ).then((items) => {
      if (!active) {
        return;
      }
      setFetchedProcessMessages(items);
      setLoading(false);
}).catch((fetchError) => {
      if (!active) {
        return;
      }
      setFetchedProcessMessages([]);
      setLoading(false);
      setError(fetchError instanceof Error ? fetchError.message : t('turnProcess.loadErrorFallback'));
    });

    return () => {
      active = false;
    };
  }, [apiClient, normalizedUserMessageId, open, resolvedTurnId, sessionId, t]);

  const processMessages = useMemo(
    () => mergeUniqueMessages(
      mergeUniqueMessages(fetchedProcessMessages, cachedProcessMessagesForTurn),
      localProcessMessages,
    ),
    [cachedProcessMessagesForTurn, fetchedProcessMessages, localProcessMessages],
  );

  const effectiveAssistant = streamingFinalAssistant || finalAssistantMessage;

  const timelineItems = useMemo<TurnProcessTimelineItem[]>(() => buildTurnProcessTimeline({
    processMessages,
    fallbackAssistantMessage: effectiveAssistant,
  }), [effectiveAssistant, processMessages]);

  const stats = useMemo(() => {
    const historyProcess = userMessage?.metadata?.historyProcess;
    const timelineToolCount = timelineItems.filter((item) => item.kind === 'tool_call').length;
    const timelineThinkingCount = timelineItems.filter((item) => item.kind === 'thinking').length;
    const timelineUnavailableCount = timelineItems.filter((item) => item.kind === 'tool_unavailable').length;
    return {
      toolCount: Math.max(Number(historyProcess?.toolCallCount || 0), timelineToolCount),
      thinkingCount: Math.max(Number(historyProcess?.thinkingCount || 0), timelineThinkingCount),
      unavailableCount: Math.max(Number(historyProcess?.unavailableToolCount || 0), timelineUnavailableCount),
      processMessageCount: Math.max(Number(historyProcess?.processMessageCount || 0), processMessages.length),
    };
  }, [processMessages.length, timelineItems, userMessage]);

  const isStreaming = Boolean(streamingFinalAssistant);

  return {
    userMessage,
    resolvedTurnId,
    finalAssistantMessage: effectiveAssistant,
    processMessages,
    timelineItems,
    stats,
    loading,
    error,
    isStreaming,
  };
};
