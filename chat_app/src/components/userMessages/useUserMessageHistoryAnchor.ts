import { useCallback, useEffect, useRef, useState } from 'react';

import type { Message } from '../../types';

interface UseUserMessageHistoryAnchorParams {
  sessionId: string | null | undefined;
  messages: Message[];
  hasMoreMessages: boolean;
  onLoadMore: () => void | Promise<void>;
}

export const useUserMessageHistoryAnchor = ({
  sessionId,
  messages,
  hasMoreMessages,
  onLoadMore,
}: UseUserMessageHistoryAnchorParams) => {
  const [anchorMessageId, setAnchorMessageId] = useState<string | null>(null);
  const [anchorRequestKey, setAnchorRequestKey] = useState(0);
  const [pendingAnchorMessageId, setPendingAnchorMessageId] = useState<string | null>(null);
  const [pendingHistorySyncMessageId, setPendingHistorySyncMessageId] = useState<string | null>(null);
  const [rightHistoryLoadTick, setRightHistoryLoadTick] = useState(0);
  const rightHistoryLoadInFlightRef = useRef<Promise<void> | null>(null);
  const anchorLoadAttemptRef = useRef(0);
  const historySyncAttemptRef = useRef(0);
  const activeSessionId = sessionId || null;

  const loadRightHistoryPage = useCallback(() => {
    if (rightHistoryLoadInFlightRef.current) {
      return rightHistoryLoadInFlightRef.current;
    }
    const request = Promise.resolve(onLoadMore()).finally(() => {
      rightHistoryLoadInFlightRef.current = null;
      setRightHistoryLoadTick((current) => current + 1);
    });
    rightHistoryLoadInFlightRef.current = request;
    return request;
  }, [onLoadMore]);

  const handleSelectUserMessage = useCallback((message: Message) => {
    const messageId = String(message.id || '').trim();
    if (!messageId) {
      return;
    }
    anchorLoadAttemptRef.current = 0;
    setAnchorMessageId(messageId);
    setAnchorRequestKey((current) => current + 1);
    setPendingAnchorMessageId(messageId);
  }, []);

  const handleLoadMoreUserMessagesHistory = useCallback((oldestLoadedMessage: Message | null) => {
    const messageId = String(oldestLoadedMessage?.id || '').trim();
    if (!messageId) {
      return;
    }
    historySyncAttemptRef.current = 0;
    setPendingHistorySyncMessageId(messageId);
  }, []);

  const handleClearAnchor = useCallback(() => {
    setAnchorMessageId(null);
    setPendingAnchorMessageId(null);
    anchorLoadAttemptRef.current = 0;
  }, []);

  useEffect(() => {
    setAnchorMessageId(null);
    setPendingAnchorMessageId(null);
    setPendingHistorySyncMessageId(null);
    anchorLoadAttemptRef.current = 0;
    historySyncAttemptRef.current = 0;
  }, [activeSessionId]);

  useEffect(() => {
    const messageId = String(pendingHistorySyncMessageId || '').trim();
    if (!messageId || !activeSessionId) {
      return;
    }
    if ((messages || []).some((message) => message.id === messageId)) {
      setPendingHistorySyncMessageId(null);
      return;
    }
    if (!hasMoreMessages || historySyncAttemptRef.current >= 20) {
      setPendingHistorySyncMessageId(null);
      return;
    }
    if (rightHistoryLoadInFlightRef.current) {
      return;
    }

    historySyncAttemptRef.current += 1;
    void loadRightHistoryPage();
  }, [
    pendingHistorySyncMessageId,
    hasMoreMessages,
    messages,
    activeSessionId,
    loadRightHistoryPage,
    rightHistoryLoadTick,
  ]);

  useEffect(() => {
    const messageId = String(pendingAnchorMessageId || '').trim();
    if (!messageId || !activeSessionId) {
      return;
    }
    if ((messages || []).some((message) => message.id === messageId)) {
      setAnchorMessageId(messageId);
      setPendingAnchorMessageId(null);
      return;
    }
    if (!hasMoreMessages) {
      setAnchorMessageId(messageId);
      setPendingAnchorMessageId(null);
      return;
    }
    if (anchorLoadAttemptRef.current >= 20) {
      setPendingAnchorMessageId(null);
      return;
    }
    if (rightHistoryLoadInFlightRef.current) {
      return;
    }

    anchorLoadAttemptRef.current += 1;
    void loadRightHistoryPage();
  }, [
    pendingAnchorMessageId,
    hasMoreMessages,
    messages,
    activeSessionId,
    loadRightHistoryPage,
    rightHistoryLoadTick,
  ]);

  return {
    anchorMessageId,
    anchorRequestKey,
    handleSelectUserMessage,
    handleLoadMoreUserMessagesHistory,
    handleClearAnchor,
  };
};
