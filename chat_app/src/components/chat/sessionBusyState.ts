type BusyChatState = {
  isLoading?: boolean;
  isStreaming?: boolean;
  streamingPhase?: 'thinking' | 'reviewing' | null;
} | null | undefined;

export const resolveSessionBusyPhase = ({
  chatState,
}: {
  chatState?: BusyChatState;
}): 'thinking' | 'reviewing' | null => {
  if (chatState?.streamingPhase === 'reviewing') {
    return 'reviewing';
  }
  if (chatState?.streamingPhase === 'thinking') {
    return 'thinking';
  }
  if (chatState?.isLoading || chatState?.isStreaming) {
    return 'thinking';
  }
  return null;
};
