import { useConversationPaneProps } from './useConversationPaneProps';
import { useOverlayDrawerProps } from './useOverlayDrawerProps';
import { useSessionListProps } from './useSessionListProps';
import type { ChatInterfaceViewPropsParams } from './viewPropsTypes';

export const useChatInterfaceViewProps = ({
  conversation,
  conversationActions,
  overlay,
  overlayActions,
}: ChatInterfaceViewPropsParams) => {
  const sessionListProps = useSessionListProps();

  const conversationPaneProps = useConversationPaneProps({
    conversation,
    actions: conversationActions,
  });

  const {
    uiPromptHistoryProps,
    runtimeContextProps,
  } = useOverlayDrawerProps({
    overlay,
    actions: overlayActions,
  });

  return {
    sessionListProps,
    conversationPaneProps,
    uiPromptHistoryProps,
    runtimeContextProps,
  };
};
