import { useMemo, type ComponentProps } from 'react';

import TurnRuntimeContextDrawer from './TurnRuntimeContextDrawer';
import UiPromptHistoryDrawer from './UiPromptHistoryDrawer';
import { formatSummaryCreatedAt } from './helpers';
import type {
  ChatInterfaceOverlayActions,
  ChatInterfaceOverlayState,
} from './viewPropsTypes';

interface UseOverlayDrawerPropsParams {
  overlay: ChatInterfaceOverlayState;
  actions: ChatInterfaceOverlayActions;
}

export const useOverlayDrawerProps = ({
  overlay,
  actions,
}: UseOverlayDrawerPropsParams) => {
  const uiPromptHistoryProps = useMemo<ComponentProps<typeof UiPromptHistoryDrawer>>(() => ({
    open: overlay.uiPromptHistoryOpen,
    items: overlay.uiPromptHistoryItems,
    loading: overlay.uiPromptHistoryLoading,
    error: overlay.uiPromptHistoryError,
    refreshDisabled: !overlay.currentSession || overlay.uiPromptHistoryLoading,
    onRefresh: () => {
      if (!overlay.currentSessionId) {
        return;
      }
      void actions.loadUiPromptHistory(overlay.currentSessionId, true);
    },
    onClose: () => actions.setUiPromptHistoryOpen(false),
    formatCreatedAt: formatSummaryCreatedAt,
  }), [actions, overlay]);

  const runtimeContextProps = useMemo<ComponentProps<typeof TurnRuntimeContextDrawer>>(() => ({
    open: overlay.runtimeContextOpen,
    sessionId: overlay.runtimeContextSessionId,
    loading: overlay.runtimeContextLoading,
    error: overlay.runtimeContextError,
    data: overlay.runtimeContextData,
    onRefresh: actions.handleRefreshRuntimeContext,
    onClose: () => actions.setRuntimeContextOpen(false),
  }), [actions, overlay]);

  return {
    uiPromptHistoryProps,
    runtimeContextProps,
  };
};
