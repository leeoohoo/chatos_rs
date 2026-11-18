import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type UIEventHandler,
} from "react";

const BOTTOM_THRESHOLD_PX = 28;

export interface TimelineMessageMarkerSource {
  message_id: string;
  ordinal: number;
  content: unknown;
  metadata: unknown;
}

interface TimelineMarker {
  messageId: string;
  ordinal: number;
  revision: string;
}

interface PendingOlderAnchor {
  scrollHeight: number;
  scrollTop: number;
}

interface ViewportState {
  conversationKey: string;
  initialized: boolean;
  pinnedToBottom: boolean;
  latestMarker: TimelineMarker | null;
}

export function useConversationTimelineViewport({
  active = true,
  conversationKey,
  isLoadingOlder,
  messages,
  onLoadOlder,
}: {
  active?: boolean;
  conversationKey: string;
  isLoadingOlder: boolean;
  messages: readonly TimelineMessageMarkerSource[];
  onLoadOlder(): void | Promise<void>;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const pendingOlderAnchorRef = useRef<PendingOlderAnchor | null>(null);
  const viewportStateRef = useRef<ViewportState>({
    conversationKey,
    initialized: false,
    pinnedToBottom: true,
    latestMarker: null,
  });
  const [unreadNewerCount, setUnreadNewerCount] = useState(0);

  useLayoutEffect(() => {
    pendingOlderAnchorRef.current = null;
    viewportStateRef.current = {
      conversationKey,
      initialized: false,
      pinnedToBottom: true,
      latestMarker: null,
    };
    setUnreadNewerCount(0);
  }, [conversationKey]);

  useLayoutEffect(() => {
    if (!active) return;
    const viewport = scrollRef.current;
    if (!viewport) return;
    const state = viewportStateRef.current;
    const latestMarker = captureLatestTimelineMarker(messages);

    if (!state.initialized) {
      scrollTimelineToBottom(viewport);
      state.initialized = true;
      state.pinnedToBottom = true;
      state.latestMarker = latestMarker;
      return;
    }

    const olderAnchor = pendingOlderAnchorRef.current;
    if (olderAnchor && !isLoadingOlder) {
      viewport.scrollTop = restoredScrollTopAfterPrepend(olderAnchor, viewport.scrollHeight);
      state.pinnedToBottom = isTimelinePinnedToBottom(viewport);
      state.latestMarker = latestMarker;
      pendingOlderAnchorRef.current = null;
      return;
    }

    const newerCount = countNewerTimelineUpdates(state.latestMarker, messages);
    if (state.pinnedToBottom) {
      scrollTimelineToBottom(viewport);
      setUnreadNewerCount(0);
    } else if (newerCount > 0) {
      setUnreadNewerCount((current) => current + newerCount);
    }
    state.latestMarker = latestMarker;
  }, [active, conversationKey, isLoadingOlder, messages]);

  useEffect(() => {
    if (!active) return;
    const content = contentRef.current;
    if (!content || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      const viewport = scrollRef.current;
      const state = viewportStateRef.current;
      if (!viewport || !state.pinnedToBottom || pendingOlderAnchorRef.current) return;
      scrollTimelineToBottom(viewport);
    });
    observer.observe(content);
    return () => observer.disconnect();
  }, [active, conversationKey]);

  const onScroll = useCallback<UIEventHandler<HTMLDivElement>>((event) => {
    const pinnedToBottom = isTimelinePinnedToBottom(event.currentTarget);
    viewportStateRef.current.pinnedToBottom = pinnedToBottom;
    if (pinnedToBottom) setUnreadNewerCount(0);
  }, []);

  const requestOlder = useCallback(() => {
    if (isLoadingOlder) return;
    const viewport = scrollRef.current;
    if (viewport && !pendingOlderAnchorRef.current) {
      pendingOlderAnchorRef.current = {
        scrollHeight: viewport.scrollHeight,
        scrollTop: viewport.scrollTop,
      };
    }
    void onLoadOlder();
  }, [isLoadingOlder, onLoadOlder]);

  const jumpToLatest = useCallback(() => {
    const viewport = scrollRef.current;
    if (!viewport) return;
    scrollTimelineToBottom(viewport);
    viewportStateRef.current.pinnedToBottom = true;
    setUnreadNewerCount(0);
  }, []);

  return {
    contentRef,
    jumpToLatest,
    onScroll,
    requestOlder,
    scrollRef,
    unreadNewerCount,
  };
}

export function isTimelinePinnedToBottom({
  clientHeight,
  scrollHeight,
  scrollTop,
}: Pick<HTMLElement, "clientHeight" | "scrollHeight" | "scrollTop">) {
  return scrollHeight - scrollTop - clientHeight <= BOTTOM_THRESHOLD_PX;
}

export function restoredScrollTopAfterPrepend(
  anchor: PendingOlderAnchor,
  nextScrollHeight: number,
) {
  return Math.max(0, anchor.scrollTop + nextScrollHeight - anchor.scrollHeight);
}

function scrollTimelineToBottom(viewport: HTMLDivElement) {
  viewport.scrollTop = Math.max(0, viewport.scrollHeight - viewport.clientHeight);
}

function captureLatestTimelineMarker(
  messages: readonly TimelineMessageMarkerSource[],
): TimelineMarker | null {
  const latest = messages.at(-1);
  if (!latest) return null;
  return {
    messageId: latest.message_id,
    ordinal: latest.ordinal,
    revision: messageRevision(latest),
  };
}

function countNewerTimelineUpdates(
  previous: TimelineMarker | null,
  messages: readonly TimelineMessageMarkerSource[],
) {
  if (!previous) return 0;
  const appendedCount = messages.reduce(
    (count, message) => count + (message.ordinal > previous.ordinal ? 1 : 0),
    0,
  );
  if (appendedCount > 0) return appendedCount;
  const latest = messages.at(-1);
  if (!latest || latest.message_id !== previous.messageId) return 0;
  return messageRevision(latest) === previous.revision ? 0 : 1;
}

function messageRevision(message: TimelineMessageMarkerSource) {
  try {
    return JSON.stringify([message.content, message.metadata]) ?? "";
  } catch {
    return String(message.content);
  }
}
