import { useMemo, type ComponentProps } from 'react';

import TurnRuntimeContextDrawer from './TurnRuntimeContextDrawer';
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
    runtimeContextProps,
  };
};
