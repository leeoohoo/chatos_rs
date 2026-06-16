import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';

import type { Message } from '../../types';

const MESSAGE_WINDOW_EXPAND_TOP_OFFSET = 120;
const ESTIMATED_MESSAGE_ROW_HEIGHT = 120;
const MESSAGE_WINDOW_MIN_SIZE = 20;
const MESSAGE_WINDOW_MAX_SIZE = 72;
const MESSAGE_WINDOW_OVERSCAN_ROWS = 6;
const MESSAGE_WINDOW_THRESHOLD_EXTRA = 8;
const SESSION_INITIAL_BOTTOM_LOCK_FRAMES = 10;
const AUTO_SCROLL_BOTTOM_THRESHOLD_PX = 96;

type ScrollSnapshot = {
  firstMessageId: string;
  lastMessageId: string;
  scrollHeight: number;
  scrollTop: number;
  sessionId: string;
};

interface UseMessageListWindowingParams {
  sessionId?: string | null;
  visibleMessages: Message[];
  isLoading: boolean;
  hasMore: boolean;
  isStreaming: boolean;
  anchorMessageId?: string | null;
  anchorRequestKey?: number;
  autoScrollToLatest?: boolean;
}

export const useMessageListWindowing = ({
  sessionId,
  visibleMessages,
  isLoading,
  hasMore,
  isStreaming,
  anchorMessageId = null,
  anchorRequestKey = 0,
  autoScrollToLatest = true,
}: UseMessageListWindowingParams) => {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const bottomRef = useRef<HTMLDivElement | null>(null);
  const scrollRafRef = useRef<number | null>(null);
  const initialScrollRafRef = useRef<number | null>(null);
  const streamEndScrollRafRef = useRef<number | null>(null);
  const streamEndSettleRafRef = useRef<number | null>(null);
  const autoScrollRafRef = useRef<number | null>(null);
  const sessionBottomLockRafRef = useRef<number | null>(null);
  const prevIsStreamingRef = useRef<boolean>(isStreaming);
  const pendingSessionInitialScrollRef = useRef<boolean>(true);
  const pendingSessionBottomLockFramesRef = useRef<number>(0);
  const expandingWindowRef = useRef(false);
  const prevVisibleCountRef = useRef(0);
  const latestAutoScrollKeyRef = useRef('');
  const scrollSnapshotRef = useRef<ScrollSnapshot | null>(null);
  const lastAnchorSuspendKeyRef = useRef('');
  const lastWindowedAnchorKeyRef = useRef('');

  const [autoScroll, setAutoScroll] = useState<boolean>(true);
  const [isAtBottom, setIsAtBottom] = useState<boolean>(true);
  const [renderStartIndex, setRenderStartIndex] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);

  const windowSize = useMemo(() => {
    const estimatedRows = Math.ceil((viewportHeight || 960) / ESTIMATED_MESSAGE_ROW_HEIGHT);
    const candidate = estimatedRows + MESSAGE_WINDOW_OVERSCAN_ROWS;
    return Math.min(MESSAGE_WINDOW_MAX_SIZE, Math.max(MESSAGE_WINDOW_MIN_SIZE, candidate));
  }, [viewportHeight]);
  const windowThreshold = windowSize + MESSAGE_WINDOW_THRESHOLD_EXTRA;
  const windowStep = Math.max(32, Math.floor(windowSize * 0.6));
  const shouldWindowMessages = visibleMessages.length > windowThreshold;
  const boundedRenderStartIndex = shouldWindowMessages
    ? Math.min(renderStartIndex, Math.max(0, visibleMessages.length - 1))
    : 0;
  const renderedMessages = useMemo(
    () => (shouldWindowMessages
      ? visibleMessages.slice(boundedRenderStartIndex)
      : visibleMessages),
    [shouldWindowMessages, visibleMessages, boundedRenderStartIndex],
  );
  const lastVisibleIndex = visibleMessages.length - 1;
  const latestAutoScrollKey = useMemo(() => {
    const latest = visibleMessages[visibleMessages.length - 1];
    if (!latest) {
      return '';
    }
    return [
      latest.id,
      latest.status || '',
      latest.content?.length || 0,
      latest.metadata?.task_runner_async?.status || '',
      latest.metadata?.task_runner_async?.last_event_status || '',
    ].join(':');
  }, [visibleMessages]);

  const isNearBottom = useCallback((element: HTMLDivElement): boolean => (
    element.scrollHeight - element.scrollTop - element.clientHeight <= AUTO_SCROLL_BOTTOM_THRESHOLD_PX
  ), []);

  useEffect(() => {
    pendingSessionInitialScrollRef.current = autoScrollToLatest;
    pendingSessionBottomLockFramesRef.current = autoScrollToLatest ? SESSION_INITIAL_BOTTOM_LOCK_FRAMES : 0;
    prevVisibleCountRef.current = 0;
    latestAutoScrollKeyRef.current = '';
    scrollSnapshotRef.current = null;
    if (initialScrollRafRef.current !== null) {
      cancelAnimationFrame(initialScrollRafRef.current);
      initialScrollRafRef.current = null;
    }
    if (sessionBottomLockRafRef.current !== null) {
      cancelAnimationFrame(sessionBottomLockRafRef.current);
      sessionBottomLockRafRef.current = null;
    }
    setIsAtBottom(autoScrollToLatest);
    setAutoScroll(autoScrollToLatest);
  }, [sessionId, autoScrollToLatest]);

  useEffect(() => {
    if (!autoScrollToLatest) {
      pendingSessionInitialScrollRef.current = false;
      return;
    }
    if (!pendingSessionInitialScrollRef.current) {
      return;
    }

    const hasRenderableMessages = visibleMessages.length > 0;

    if (!hasRenderableMessages && !isLoading && !hasMore) {
      pendingSessionInitialScrollRef.current = false;
      return;
    }

    if (!hasRenderableMessages) {
      return;
    }

    if (initialScrollRafRef.current !== null) {
      cancelAnimationFrame(initialScrollRafRef.current);
      initialScrollRafRef.current = null;
    }

    initialScrollRafRef.current = requestAnimationFrame(() => {
      initialScrollRafRef.current = null;
      const element = scrollRef.current;
      if (!element) {
        return;
      }
      element.scrollTop = element.scrollHeight;
      setIsAtBottom(true);
      setAutoScroll(true);
      pendingSessionInitialScrollRef.current = false;
    });

    return () => {
      if (initialScrollRafRef.current !== null) {
        cancelAnimationFrame(initialScrollRafRef.current);
        initialScrollRafRef.current = null;
      }
    };
  }, [sessionId, visibleMessages.length, isLoading, hasMore, isStreaming, autoScrollToLatest]);

  useEffect(() => {
    const target = scrollRef.current;
    if (!target) {
      return;
    }

    const updateHeight = () => {
      setViewportHeight((prev) => {
        const next = target.clientHeight || 0;
        return prev === next ? prev : next;
      });
    };

    updateHeight();

    if (typeof ResizeObserver === 'undefined') {
      return undefined;
    }

    const observer = new ResizeObserver(() => {
      updateHeight();
    });
    observer.observe(target);

    return () => observer.disconnect();
  }, [sessionId]);

  useEffect(() => {
    const nextCount = visibleMessages.length;
    const previousCount = prevVisibleCountRef.current;
    prevVisibleCountRef.current = nextCount;

    if (nextCount <= windowThreshold) {
      setRenderStartIndex(0);
      return;
    }

    const latestStart = Math.max(0, nextCount - windowSize);
    setRenderStartIndex((prev) => {
      if (previousCount === 0) {
        return autoScrollToLatest ? latestStart : 0;
      }
      if (prev === 0 && !isAtBottom) {
        return 0;
      }
      if (autoScrollToLatest && (isStreaming || autoScroll || isAtBottom)) {
        return latestStart;
      }
      return Math.min(prev, latestStart);
    });
  }, [visibleMessages.length, isStreaming, autoScroll, isAtBottom, windowSize, windowThreshold, autoScrollToLatest]);

  useEffect(() => {
    const nextCount = visibleMessages.length;
    if (nextCount <= windowThreshold) {
      setRenderStartIndex(0);
      return;
    }
    if (!autoScrollToLatest) {
      return;
    }
    if (!pendingSessionInitialScrollRef.current && !isStreaming && !autoScroll && !isAtBottom) {
      return;
    }
    setRenderStartIndex(Math.max(0, nextCount - windowSize));
  }, [sessionId, visibleMessages.length, windowSize, windowThreshold, isStreaming, autoScroll, isAtBottom, autoScrollToLatest]);

  useEffect(() => {
    const normalizedAnchorMessageId = String(anchorMessageId || '').trim();
    if (!normalizedAnchorMessageId) {
      lastAnchorSuspendKeyRef.current = '';
      lastWindowedAnchorKeyRef.current = '';
      return;
    }
    const anchorKey = `${anchorRequestKey}:${normalizedAnchorMessageId}`;
    if (lastAnchorSuspendKeyRef.current !== anchorKey) {
      lastAnchorSuspendKeyRef.current = anchorKey;
      pendingSessionInitialScrollRef.current = false;
      pendingSessionBottomLockFramesRef.current = 0;
      setAutoScroll(false);
      setIsAtBottom(false);
      if (initialScrollRafRef.current !== null) {
        cancelAnimationFrame(initialScrollRafRef.current);
        initialScrollRafRef.current = null;
      }
      if (autoScrollRafRef.current !== null) {
        cancelAnimationFrame(autoScrollRafRef.current);
        autoScrollRafRef.current = null;
      }
      if (sessionBottomLockRafRef.current !== null) {
        cancelAnimationFrame(sessionBottomLockRafRef.current);
        sessionBottomLockRafRef.current = null;
      }
      if (streamEndScrollRafRef.current !== null) {
        cancelAnimationFrame(streamEndScrollRafRef.current);
        streamEndScrollRafRef.current = null;
      }
      if (streamEndSettleRafRef.current !== null) {
        cancelAnimationFrame(streamEndSettleRafRef.current);
        streamEndSettleRafRef.current = null;
      }
    }
    if (!shouldWindowMessages) {
      return;
    }
    if (lastWindowedAnchorKeyRef.current === anchorKey) {
      return;
    }
    const targetIndex = visibleMessages.findIndex((message) => message.id === normalizedAnchorMessageId);
    if (targetIndex < 0) {
      return;
    }
    lastWindowedAnchorKeyRef.current = anchorKey;
    const latestStart = Math.max(0, visibleMessages.length - windowSize);
    setRenderStartIndex((current) => (
      targetIndex >= current
        ? current
        : Math.max(0, Math.min(targetIndex, latestStart))
    ));
  }, [anchorMessageId, anchorRequestKey, shouldWindowMessages, visibleMessages, windowSize]);

  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) {
      scrollSnapshotRef.current = null;
      return;
    }

    const firstMessageId = visibleMessages[0]?.id || '';
    const lastMessageId = visibleMessages[visibleMessages.length - 1]?.id || '';
    const previous = scrollSnapshotRef.current;

    if (
      previous
      && previous.sessionId === (sessionId || '')
      && !pendingSessionInitialScrollRef.current
      && firstMessageId
      && previous.firstMessageId
      && firstMessageId !== previous.firstMessageId
    ) {
      const previousFirstIndex = visibleMessages.findIndex((message) => message.id === previous.firstMessageId);
      const sameTail = !previous.lastMessageId || previous.lastMessageId === lastMessageId;
      if (previousFirstIndex > 0 && sameTail) {
        const heightDelta = element.scrollHeight - previous.scrollHeight;
        if (heightDelta !== 0) {
          element.scrollTop = previous.scrollTop + heightDelta;
        }
      }
    }

    scrollSnapshotRef.current = {
      firstMessageId,
      lastMessageId,
      scrollHeight: element.scrollHeight,
      scrollTop: element.scrollTop,
      sessionId: sessionId || '',
    };
  }, [sessionId, visibleMessages, renderedMessages.length, boundedRenderStartIndex]);

  const expandRenderedWindow = useCallback(() => {
    if (!shouldWindowMessages || boundedRenderStartIndex <= 0) {
      return;
    }

    const scroller = scrollRef.current;
    if (expandingWindowRef.current) {
      return;
    }
    if (!scroller) {
      setRenderStartIndex((prev) => Math.max(0, prev - windowStep));
      return;
    }

    expandingWindowRef.current = true;
    const prevScrollHeight = scroller.scrollHeight;
    setRenderStartIndex((prev) => Math.max(0, prev - windowStep));

    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        const nextScroller = scrollRef.current;
        if (nextScroller) {
          const delta = nextScroller.scrollHeight - prevScrollHeight;
          if (delta > 0) {
            nextScroller.scrollTop += delta;
          }
        }
        expandingWindowRef.current = false;
      });
    });
  }, [boundedRenderStartIndex, shouldWindowMessages, windowStep]);

  const cancelPendingStreamEndScroll = useCallback(() => {
    if (streamEndScrollRafRef.current !== null) {
      cancelAnimationFrame(streamEndScrollRafRef.current);
      streamEndScrollRafRef.current = null;
    }
    if (streamEndSettleRafRef.current !== null) {
      cancelAnimationFrame(streamEndSettleRafRef.current);
      streamEndSettleRafRef.current = null;
    }
  }, []);

  const scrollToBottom = useCallback((smooth = true) => {
    const element = scrollRef.current;
    if (!element) {
      return;
    }
    setIsAtBottom(true);
    if (smooth) {
      element.scrollTo({ top: element.scrollHeight, behavior: 'smooth' });
      return;
    }
    element.scrollTop = element.scrollHeight;
  }, []);

  const scheduleAutoScrollToBottom = useCallback(() => {
    if (!autoScrollToLatest) {
      return;
    }
    if (autoScrollRafRef.current !== null) {
      return;
    }
    autoScrollRafRef.current = requestAnimationFrame(() => {
      autoScrollRafRef.current = null;
      scrollToBottom(false);
    });
  }, [scrollToBottom, autoScrollToLatest]);

  const scheduleSessionBottomLock = useCallback(() => {
    if (!autoScrollToLatest) {
      return;
    }
    if (pendingSessionBottomLockFramesRef.current <= 0) {
      return;
    }
    if (sessionBottomLockRafRef.current !== null) {
      return;
    }
    sessionBottomLockRafRef.current = requestAnimationFrame(() => {
      sessionBottomLockRafRef.current = null;
      if (pendingSessionBottomLockFramesRef.current <= 0) {
        return;
      }
      scrollToBottom(false);
      setIsAtBottom(true);
      pendingSessionBottomLockFramesRef.current -= 1;
      if (pendingSessionBottomLockFramesRef.current > 0) {
        scheduleSessionBottomLock();
      }
    });
  }, [scrollToBottom, autoScrollToLatest]);

  const scheduleStreamEndBottomLock = useCallback((frames = 8) => {
    if (!autoScrollToLatest) {
      return;
    }
    cancelPendingStreamEndScroll();
    if (frames <= 0) {
      return;
    }

    const lockBottom = (remaining: number) => {
      scrollToBottom(false);
      setIsAtBottom(true);
      if (remaining <= 1) {
        return;
      }
      streamEndSettleRafRef.current = requestAnimationFrame(() => {
        streamEndSettleRafRef.current = null;
        lockBottom(remaining - 1);
      });
    };

    streamEndScrollRafRef.current = requestAnimationFrame(() => {
      streamEndScrollRafRef.current = null;
      lockBottom(frames);
    });
  }, [cancelPendingStreamEndScroll, scrollToBottom, autoScrollToLatest]);

  useEffect(() => {
    const root = scrollRef.current;
    const target = bottomRef.current;
    if (!root || !target || typeof IntersectionObserver === 'undefined') {
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        const atBottom = Boolean(entries[0]?.isIntersecting);
        setIsAtBottom((prev) => (prev === atBottom ? prev : atBottom));
        if (!atBottom) {
          setAutoScroll((prev) => (prev ? false : prev));
          return;
        }
        if (autoScrollToLatest && isStreaming) {
          setAutoScroll((prev) => (prev ? prev : true));
        }
      },
      {
        root,
        threshold: 0.98,
      },
    );

    observer.observe(target);
    return () => observer.disconnect();
  }, [sessionId, isStreaming, renderedMessages.length, autoScrollToLatest]);

  useLayoutEffect(() => {
    if (!autoScrollToLatest) {
      latestAutoScrollKeyRef.current = latestAutoScrollKey;
      return;
    }
    if (pendingSessionInitialScrollRef.current) {
      latestAutoScrollKeyRef.current = latestAutoScrollKey;
      return;
    }

    const previousKey = latestAutoScrollKeyRef.current;
    latestAutoScrollKeyRef.current = latestAutoScrollKey;
    if (!latestAutoScrollKey || !previousKey || latestAutoScrollKey === previousKey) {
      return;
    }

    if (autoScroll || isAtBottom || isStreaming) {
      scheduleAutoScrollToBottom();
    }
  }, [
    latestAutoScrollKey,
    isStreaming,
    autoScroll,
    isAtBottom,
    scheduleAutoScrollToBottom,
    autoScrollToLatest,
  ]);

  useEffect(() => {
    if (!autoScrollToLatest) {
      return;
    }
    if (pendingSessionInitialScrollRef.current) {
      return;
    }
    if (pendingSessionBottomLockFramesRef.current <= 0) {
      return;
    }
    scheduleSessionBottomLock();
  }, [
    sessionId,
    visibleMessages.length,
    viewportHeight,
    scheduleSessionBottomLock,
    autoScrollToLatest,
  ]);

  useEffect(() => {
    const wasStreaming = prevIsStreamingRef.current;
    prevIsStreamingRef.current = isStreaming;
    if (!wasStreaming || isStreaming) {
      return;
    }

    if (!autoScrollToLatest || !(autoScroll || isAtBottom)) {
      return;
    }

    scheduleStreamEndBottomLock();

    return cancelPendingStreamEndScroll;
  }, [
    isStreaming,
    autoScroll,
    isAtBottom,
    scheduleStreamEndBottomLock,
    cancelPendingStreamEndScroll,
    autoScrollToLatest,
  ]);

  useEffect(() => {
    if (autoScrollToLatest && isStreaming && isAtBottom) {
      setAutoScroll((prev) => (prev ? prev : true));
    }
  }, [isStreaming, isAtBottom, autoScrollToLatest]);

  const handleScroll = useCallback(() => {
    if (scrollRafRef.current !== null) {
      return;
    }
    scrollRafRef.current = requestAnimationFrame(() => {
      scrollRafRef.current = null;
      const element = scrollRef.current;
      if (!element) {
        return;
      }
      const nearBottom = isNearBottom(element);
      setIsAtBottom((prev) => (prev === nearBottom ? prev : nearBottom));
      setAutoScroll((prev) => {
        const next = autoScrollToLatest ? nearBottom : false;
        return prev === next ? prev : next;
      });
      if (
        shouldWindowMessages
        &&
        boundedRenderStartIndex > 0
        && element.scrollTop <= MESSAGE_WINDOW_EXPAND_TOP_OFFSET
      ) {
        expandRenderedWindow();
      }
    });
  }, [boundedRenderStartIndex, expandRenderedWindow, isNearBottom, shouldWindowMessages, autoScrollToLatest]);

  const handleJumpToBottom = useCallback(() => {
    scrollToBottom(true);
    setAutoScroll(autoScrollToLatest);
  }, [scrollToBottom, autoScrollToLatest]);

  useEffect(() => {
    return () => {
      if (scrollRafRef.current !== null) {
        cancelAnimationFrame(scrollRafRef.current);
        scrollRafRef.current = null;
      }
      if (initialScrollRafRef.current !== null) {
        cancelAnimationFrame(initialScrollRafRef.current);
        initialScrollRafRef.current = null;
      }
      if (streamEndScrollRafRef.current !== null) {
        cancelAnimationFrame(streamEndScrollRafRef.current);
        streamEndScrollRafRef.current = null;
      }
      if (streamEndSettleRafRef.current !== null) {
        cancelAnimationFrame(streamEndSettleRafRef.current);
        streamEndSettleRafRef.current = null;
      }
      if (autoScrollRafRef.current !== null) {
        cancelAnimationFrame(autoScrollRafRef.current);
        autoScrollRafRef.current = null;
      }
      if (sessionBottomLockRafRef.current !== null) {
        cancelAnimationFrame(sessionBottomLockRafRef.current);
        sessionBottomLockRafRef.current = null;
      }
      expandingWindowRef.current = false;
    };
  }, []);

  return {
    scrollRef,
    bottomRef,
    renderedMessages,
    shouldWindowMessages,
    boundedRenderStartIndex,
    lastVisibleIndex,
    isAtBottom,
    expandRenderedWindow,
    handleScroll,
    handleJumpToBottom,
  };
};
