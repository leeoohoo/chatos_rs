// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useApiClient } from '../../../lib/api/ApiClientContext';
import { normalizePersistedMessage } from '../../../lib/store/actions/sendMessage/persistedTurnMessages';
import type { Message } from '../../../types';
import {
  buildTimelineItems,
  isProcessMessage,
  type TimelineItem,
} from '../../userMessages/ConversationProcessTimelineModel';

const PLANNER_PROCESS_REFRESH_INTERVAL_MS = 2_000;

export interface RequirementExecutionPlannerTimelineState {
  error: string | null;
  items: TimelineItem[];
  loading: boolean;
  processMessageCount: number;
  refresh: (silent?: boolean) => Promise<void>;
}

export const isRequirementExecutionPlannerTimelineMessage = (message: Message): boolean => (
  isProcessMessage(message)
  || message.role === 'assistant'
  || message.role === 'tool'
);

export const useRequirementExecutionPlannerTimeline = ({
  active,
  conversationId,
  turnId,
  userMessageId,
}: {
  active: boolean;
  conversationId: string;
  turnId: string;
  userMessageId: string;
}): RequirementExecutionPlannerTimelineState => {
  const apiClient = useApiClient();
  const apiClientRef = useRef(apiClient);
  const [messages, setMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const requestSequenceRef = useRef(0);

  useEffect(() => {
    apiClientRef.current = apiClient;
  }, [apiClient]);

  const refresh = useCallback(async (silent = false) => {
    const requestSequence = requestSequenceRef.current + 1;
    requestSequenceRef.current = requestSequence;
    if (!silent) setLoading(true);
    try {
      const response = await apiClientRef.current.getConversationTurnMessagesByTurn(
        conversationId,
        turnId,
      );
      if (requestSequenceRef.current !== requestSequence) return;
      const normalized = (Array.isArray(response) ? response : [])
        .map((rawMessage) => normalizePersistedMessage(rawMessage, conversationId))
        .filter((message): message is Message => message !== null);
      setMessages(normalized);
      setError(null);
    } catch (err) {
      if (requestSequenceRef.current !== requestSequence) return;
      setError(err instanceof Error ? err.message : '读取规划运行过程失败');
    } finally {
      if (requestSequenceRef.current === requestSequence) {
        setLoading(false);
      }
    }
  }, [conversationId, turnId]);

  useEffect(() => {
    requestSequenceRef.current += 1;
    setMessages([]);
    setError(null);
    setLoading(true);
    void refresh(false);
  }, [conversationId, refresh, turnId]);

  useEffect(() => {
    if (!active || !conversationId || !turnId || !userMessageId) return undefined;
    const intervalId = window.setInterval(() => {
      void refresh(true);
    }, PLANNER_PROCESS_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(intervalId);
  }, [active, conversationId, refresh, turnId, userMessageId]);

  const processMessages = useMemo(
    () => messages.filter(isRequirementExecutionPlannerTimelineMessage),
    [messages],
  );
  const items = useMemo(
    () => buildTimelineItems(processMessages),
    [processMessages],
  );

  return {
    error,
    items,
    loading,
    processMessageCount: processMessages.length,
    refresh,
  };
};
