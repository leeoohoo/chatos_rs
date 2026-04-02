import { useCallback } from 'react';
import type {
  TaskReviewDraft,
  TaskReviewPanelState,
  UiPromptPanelState,
  UiPromptResponsePayload,
} from '../../lib/store/types';

interface PanelActionsApiClient {
  submitTaskReviewDecision: (
    reviewId: string,
    payload: {
      action: 'confirm' | 'cancel';
      tasks?: Array<{
        title: string;
        details: string;
        priority: TaskReviewDraft['priority'];
        status: TaskReviewDraft['status'];
        tags: string[];
        due_at?: string | null;
        planned_builtin_mcp_ids?: string[];
        planned_context_assets?: Array<{
          asset_type: string;
          asset_id: string;
          display_name?: string | null;
          source_type?: string | null;
          source_path?: string | null;
        }>;
        execution_result_contract?: {
          result_required: boolean;
          preferred_format?: string | null;
        } | null;
      }>;
      reason?: string;
    },
  ) => Promise<unknown>;
  submitUiPromptResponse: (
    promptId: string,
    payload: UiPromptResponsePayload,
  ) => Promise<unknown>;
}

interface UsePanelActionsArgs {
  activeTaskReviewPanel: TaskReviewPanelState | null;
  activeUiPromptPanel: UiPromptPanelState | null;
  apiClient: PanelActionsApiClient;
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
  loadCurrentTurnWorkbarTasks: (sessionId: string, conversationTurnId?: string | null) => Promise<void>;
  loadHistoryWorkbarTasks: (sessionId: string, force?: boolean) => Promise<void>;
  loadWorkbarSummaries: (sessionId: string, force?: boolean) => Promise<void>;
  loadUiPromptHistory: (sessionId: string, force?: boolean) => Promise<void>;
}

export function usePanelActions({
  activeTaskReviewPanel,
  activeUiPromptPanel,
  apiClient,
  upsertTaskReviewPanel,
  removeTaskReviewPanel,
  upsertUiPromptPanel,
  removeUiPromptPanel,
  loadCurrentTurnWorkbarTasks,
  loadHistoryWorkbarTasks,
  loadWorkbarSummaries,
  loadUiPromptHistory,
}: UsePanelActionsArgs) {
  const handleTaskReviewConfirm = useCallback(async (drafts: TaskReviewDraft[]) => {
    if (!activeTaskReviewPanel) {
      return;
    }

    const pendingPanel = {
      ...activeTaskReviewPanel,
      drafts,
      submitting: true,
      error: null,
    };
    upsertTaskReviewPanel(pendingPanel);

    try {
      await apiClient.submitTaskReviewDecision(activeTaskReviewPanel.reviewId, {
        action: 'confirm',
        tasks: drafts.map((draft) => ({
          title: draft.title,
          details: draft.details,
          priority: draft.priority,
          status: draft.status,
          tags: draft.tags,
          due_at: draft.dueAt || undefined,
          planned_builtin_mcp_ids: draft.plannedBuiltinMcpIds || [],
          planned_context_assets: (draft.plannedContextAssets || []).map((asset) => ({
            asset_type: asset.assetType,
            asset_id: asset.assetId,
            display_name: asset.displayName || undefined,
            source_type: asset.sourceType || undefined,
            source_path: asset.sourcePath || undefined,
          })),
          execution_result_contract: draft.executionResultContract
            ? {
              result_required: draft.executionResultContract.resultRequired !== false,
              preferred_format: draft.executionResultContract.preferredFormat || undefined,
            }
            : undefined,
        })),
      });
      removeTaskReviewPanel(activeTaskReviewPanel.reviewId, activeTaskReviewPanel.sessionId);
      await Promise.all([
        loadCurrentTurnWorkbarTasks(activeTaskReviewPanel.sessionId, activeTaskReviewPanel.conversationTurnId),
        loadHistoryWorkbarTasks(activeTaskReviewPanel.sessionId, true),
        loadWorkbarSummaries(activeTaskReviewPanel.sessionId, true),
      ]);
    } catch (error) {
      const message = error instanceof Error ? error.message : '任务确认提交失败';
      upsertTaskReviewPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [
    activeTaskReviewPanel,
    apiClient,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    loadWorkbarSummaries,
    removeTaskReviewPanel,
    upsertTaskReviewPanel,
  ]);

  const handleTaskReviewCancel = useCallback(async () => {
    if (!activeTaskReviewPanel) {
      return;
    }

    const pendingPanel = {
      ...activeTaskReviewPanel,
      submitting: true,
      error: null,
    };
    upsertTaskReviewPanel(pendingPanel);

    try {
      await apiClient.submitTaskReviewDecision(activeTaskReviewPanel.reviewId, {
        action: 'cancel',
        reason: 'user_cancelled',
      });
      removeTaskReviewPanel(activeTaskReviewPanel.reviewId, activeTaskReviewPanel.sessionId);
      await Promise.all([
        loadCurrentTurnWorkbarTasks(activeTaskReviewPanel.sessionId, activeTaskReviewPanel.conversationTurnId),
        loadHistoryWorkbarTasks(activeTaskReviewPanel.sessionId, true),
        loadWorkbarSummaries(activeTaskReviewPanel.sessionId, true),
      ]);
    } catch (error) {
      const message = error instanceof Error ? error.message : '任务取消提交失败';
      upsertTaskReviewPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [
    activeTaskReviewPanel,
    apiClient,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    loadWorkbarSummaries,
    removeTaskReviewPanel,
    upsertTaskReviewPanel,
  ]);

  const handleUiPromptSubmit = useCallback(async (payload: UiPromptResponsePayload) => {
    if (!activeUiPromptPanel) {
      return;
    }

    const pendingPanel = {
      ...activeUiPromptPanel,
      submitting: true,
      error: null,
    };
    upsertUiPromptPanel(pendingPanel);

    try {
      await apiClient.submitUiPromptResponse(activeUiPromptPanel.promptId, payload);
      removeUiPromptPanel(activeUiPromptPanel.promptId, activeUiPromptPanel.sessionId);
      await loadUiPromptHistory(activeUiPromptPanel.sessionId, true);
    } catch (error) {
      const message = error instanceof Error ? error.message : '交互确认提交失败';
      upsertUiPromptPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [
    activeUiPromptPanel,
    apiClient,
    loadUiPromptHistory,
    removeUiPromptPanel,
    upsertUiPromptPanel,
  ]);

  const handleUiPromptCancel = useCallback(async () => {
    if (!activeUiPromptPanel) {
      return;
    }

    const pendingPanel = {
      ...activeUiPromptPanel,
      submitting: true,
      error: null,
    };
    upsertUiPromptPanel(pendingPanel);

    try {
      await apiClient.submitUiPromptResponse(activeUiPromptPanel.promptId, {
        status: 'canceled',
        reason: 'user_cancelled',
      });
      removeUiPromptPanel(activeUiPromptPanel.promptId, activeUiPromptPanel.sessionId);
      await loadUiPromptHistory(activeUiPromptPanel.sessionId, true);
    } catch (error) {
      const message = error instanceof Error ? error.message : '交互确认取消失败';
      upsertUiPromptPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [
    activeUiPromptPanel,
    apiClient,
    loadUiPromptHistory,
    removeUiPromptPanel,
    upsertUiPromptPanel,
  ]);

  return {
    handleTaskReviewConfirm,
    handleTaskReviewCancel,
    handleUiPromptSubmit,
    handleUiPromptCancel,
  };
}
