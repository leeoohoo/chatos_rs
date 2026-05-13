import { useEffect, useRef } from 'react';

import type ApiClient from '../../lib/api/client';
import { useRealtimeEvent, useRealtimeTopics } from '../../lib/realtime/RealtimeProvider';
import type {
  RealtimeEventEnvelope,
  RealtimeTaskBoardPayloadWrapper,
  RealtimeUiPromptPayloadWrapper,
} from '../../lib/realtime/types';
import type { Session } from '../../types';
import type { TaskReviewPanelState, UiPromptPanelState } from '../../lib/store/types';
import {
  toTaskReviewPanelFromRealtimePayload,
  toUiPromptPanelFromRealtimePayload,
} from './panelTransforms';
import {
  removePendingTaskReviewCachePanel,
  upsertPendingTaskReviewCachePanel,
} from './pendingTaskReviewCache';
import {
  removePendingUiPromptCachePanel,
  upsertPendingUiPromptCachePanel,
} from './pendingUiPromptCache';

interface UseGlobalConversationPanelsRealtimeOptions {
  apiClient: ApiClient;
  enabled?: boolean;
  sessions?: Session[];
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
}

const isTaskBoardPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeTaskBoardPayloadWrapper } => (
  envelope?.payload?.kind === 'task_board'
);

const isUiPromptPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeUiPromptPayloadWrapper } => (
  envelope?.payload?.kind === 'ui_prompt'
);

export const useGlobalConversationPanelsRealtime = ({
  apiClient,
  enabled = true,
  sessions = [],
  upsertTaskReviewPanel,
  removeTaskReviewPanel,
  upsertUiPromptPanel,
  removeUiPromptPanel,
}: UseGlobalConversationPanelsRealtimeOptions) => {
  const upsertTaskReviewPanelRef = useRef(upsertTaskReviewPanel);
  const removeTaskReviewPanelRef = useRef(removeTaskReviewPanel);
  const upsertUiPromptPanelRef = useRef(upsertUiPromptPanel);
  const removeUiPromptPanelRef = useRef(removeUiPromptPanel);

  useEffect(() => {
    upsertTaskReviewPanelRef.current = upsertTaskReviewPanel;
  }, [upsertTaskReviewPanel]);

  useEffect(() => {
    removeTaskReviewPanelRef.current = removeTaskReviewPanel;
  }, [removeTaskReviewPanel]);

  useEffect(() => {
    upsertUiPromptPanelRef.current = upsertUiPromptPanel;
  }, [upsertUiPromptPanel]);

  useEffect(() => {
    removeUiPromptPanelRef.current = removeUiPromptPanel;
  }, [removeUiPromptPanel]);

  useRealtimeTopics(
    (sessions || []).map((session) => (
      session?.id ? { scope: 'conversation', id: session.id } : null
    )),
    enabled,
  );

  useRealtimeEvent((event) => {
    if (!enabled) {
      return;
    }

    if (event.event === 'conversation.task_board.updated' && isTaskBoardPayload(event)) {
      if (event.payload.action === 'review_required') {
        const panel = toTaskReviewPanelFromRealtimePayload(event.payload);
        if (panel) {
          upsertPendingTaskReviewCachePanel(apiClient, panel);
          upsertTaskReviewPanelRef.current(panel);
        }
        return;
      }

      if (
        event.payload.action === 'review_confirmed'
        || event.payload.action === 'review_cancelled'
      ) {
        const reviewId = typeof event.payload.review_id === 'string'
          ? event.payload.review_id.trim()
          : '';
        const sessionId = typeof event.payload.conversation_id === 'string'
          ? event.payload.conversation_id.trim()
          : '';
        if (reviewId) {
          removePendingTaskReviewCachePanel(apiClient, reviewId, sessionId || undefined);
          removeTaskReviewPanelRef.current(reviewId, sessionId || undefined);
        }
      }
      return;
    }

    if (event.event === 'conversation.ui_prompt.updated' && isUiPromptPayload(event)) {
      if (event.payload.action === 'prompt_required') {
        const panel = toUiPromptPanelFromRealtimePayload(event.payload);
        if (panel) {
          upsertPendingUiPromptCachePanel(apiClient, panel);
          upsertUiPromptPanelRef.current(panel);
        }
        return;
      }

      if (event.payload.action === 'prompt_resolved') {
        const promptId = typeof event.payload.prompt_id === 'string'
          ? event.payload.prompt_id.trim()
          : '';
        const sessionId = typeof event.payload.conversation_id === 'string'
          ? event.payload.conversation_id.trim()
          : '';
        if (promptId) {
          removePendingUiPromptCachePanel(apiClient, promptId, sessionId || undefined);
          removeUiPromptPanelRef.current(promptId, sessionId || undefined);
        }
      }
    }
  });
};
