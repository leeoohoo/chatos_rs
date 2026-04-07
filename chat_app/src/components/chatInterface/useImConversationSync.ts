import { useEffect } from 'react';

import type ApiClient from '../../lib/api/client';
import type { ImConversationActionRequestResponse } from '../../lib/api/client/types';
import type { TaskReviewPanelState, UiPromptPanelState } from '../../lib/store/types';
import {
  toTaskReviewPanelFromImActionRequest,
  toUiPromptPanelFromImActionRequest,
  toUiPromptPanelFromRecord,
} from './helpers';

const isPendingActionRequest = (record: ImConversationActionRequestResponse): boolean => (
  String(record?.status || '').trim().toLowerCase() === 'pending'
);

interface UseImConversationSyncOptions {
  apiClient: ApiClient;
  activePanel: string;
  currentSessionId: string | null;
  currentImConversationId: string | null;
  fallbackTurnId: string;
  taskReviewPanels: TaskReviewPanelState[];
  uiPromptPanels: UiPromptPanelState[];
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
}

export const useImConversationSync = ({
  apiClient,
  activePanel,
  currentSessionId,
  currentImConversationId,
  fallbackTurnId,
  taskReviewPanels,
  uiPromptPanels,
  upsertTaskReviewPanel,
  removeTaskReviewPanel,
  upsertUiPromptPanel,
  removeUiPromptPanel,
}: UseImConversationSyncOptions) => {
  useEffect(() => {
    if (!currentSessionId || activePanel !== 'chat' || currentImConversationId) {
      return;
    }

    let cancelled = false;
    void apiClient
      .getPendingUiPrompts(currentSessionId, { limit: 50 })
      .then((records) => {
        if (cancelled || !Array.isArray(records)) {
          return;
        }
        records.forEach((record) => {
          const panel = toUiPromptPanelFromRecord(record);
          if (panel) {
            upsertUiPromptPanel(panel);
          }
        });
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [
    activePanel,
    apiClient,
    currentImConversationId,
    currentSessionId,
    upsertUiPromptPanel,
  ]);

  useEffect(() => {
    if (!currentSessionId || activePanel !== 'chat' || !currentImConversationId) {
      return;
    }

    void apiClient.markImConversationRead(currentImConversationId).catch(() => {});
  }, [activePanel, apiClient, currentImConversationId, currentSessionId]);

  useEffect(() => {
    if (!currentSessionId || activePanel !== 'chat' || !currentImConversationId) {
      return;
    }

    let cancelled = false;
    void apiClient.getImConversationActionRequests(currentImConversationId)
      .then((records) => {
        if (cancelled || !Array.isArray(records)) {
          return;
        }

        const pendingTaskReviewIds = new Set<string>();
        const pendingUiPromptIds = new Set<string>();

        records
          .filter(isPendingActionRequest)
          .forEach((record) => {
            const taskPanel = toTaskReviewPanelFromImActionRequest(
              record,
              currentSessionId,
              fallbackTurnId,
            );
            if (taskPanel) {
              pendingTaskReviewIds.add(taskPanel.reviewId);
              upsertTaskReviewPanel(taskPanel);
              return;
            }

            const uiPromptPanel = toUiPromptPanelFromImActionRequest(
              record,
              currentSessionId,
              fallbackTurnId,
            );
            if (uiPromptPanel) {
              pendingUiPromptIds.add(uiPromptPanel.promptId);
              upsertUiPromptPanel(uiPromptPanel);
            }
          });

        taskReviewPanels.forEach((panel) => {
          if (panel?.source !== 'im') {
            return;
          }
          if (!pendingTaskReviewIds.has(panel.reviewId)) {
            removeTaskReviewPanel(panel.reviewId, currentSessionId);
          }
        });

        uiPromptPanels.forEach((panel) => {
          if (panel?.source !== 'im') {
            return;
          }
          if (!pendingUiPromptIds.has(panel.promptId)) {
            removeUiPromptPanel(panel.promptId, currentSessionId);
          }
        });
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [
    activePanel,
    apiClient,
    currentImConversationId,
    currentSessionId,
    fallbackTurnId,
    removeTaskReviewPanel,
    removeUiPromptPanel,
    taskReviewPanels,
    uiPromptPanels,
    upsertTaskReviewPanel,
    upsertUiPromptPanel,
  ]);
};
