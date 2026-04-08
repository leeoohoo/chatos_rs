import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { Message } from '../../types';

const MESSAGE_WINDOW_EXPAND_TOP_OFFSET = 120;
const ESTIMATED_MESSAGE_ROW_HEIGHT = 120;
const MESSAGE_WINDOW_MIN_SIZE = 20;
const MESSAGE_WINDOW_MAX_SIZE = 72;
const MESSAGE_WINDOW_OVERSCAN_ROWS = 6;
const MESSAGE_WINDOW_THRESHOLD_EXTRA = 8;

interface UseMessageListWindowingParams {
  sessionId?: string | null;
  visibleMessages: Message[];
  isLoading: boolean;
  hasMore: boolean;
  isStreaming: boolean;
}

export const useMessageListWindowing = ({
  sessionId,
  visibleMessages,
  isLoading,
  hasMore,
  isStreaming,
}: UseMessageListWindowingParams) => {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const bottomRef = useRef<HTMLDivElement | null>(null);
  const scrollRafRef = useRef<number | null>(null);
  const initialScrollRafRef = useRef<number | null>(null);
  const streamEndScrollRafRef = useRef<number | null>(null);
  const streamEndSettleRafRef = useRef<number | null>(null);
  const autoScrollRafRef = useRef<number | null>(null);
  const prevIsStreamingRef = useRef<boolean>(isStreaming);
  const pendingSessionInitialScrollRef = useRef<boolean>(true);
  const expandingWindowRef = useRef(false);
  const prevVisibleCountRef = useRef(0);

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

  useEffect(() => {
    pendingSessionInitialScrollRef.current = true;
    if (initialScrollRafRef.current !== null) {
      cancelAnimationFrame(initialScrollRafRef.current);
      initialScrollRafRef.current = null;
    }
    setIsAtBottom(true);
    setAutoScroll(false);
  }, [sessionId]);

  useEffect(() => {
    if (!pendingSessionInitialScrollRef.current) {
      return;
    }

    if (visibleMessages.length === 0 && !isLoading && !hasMore) {
      pendingSessionInitialScrollRef.current = false;
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
      setAutoScroll(isStreaming);
      pendingSessionInitialScrollRef.current = false;
    });

    return () => {
      if (initialScrollRafRef.current !== null) {
        cancelAnimationFrame(initialScrollRafRef.current);
        initialScrollRafRef.current = null;
      }
    };
  }, [sessionId, visibleMessages.length, isLoading, hasMore, isStreaming]);

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
        return latestStart;
      }
      if (prev === 0 && !isAtBottom) {
        return 0;
      }
      if (isStreaming || autoScroll || isAtBottom) {
        return latestStart;
      }
      return Math.min(prev, latestStart);
    });
  }, [visibleMessages.length, isStreaming, autoScroll, isAtBottom, windowSize, windowThreshold]);

  useEffect(() => {
    const nextCount = visibleMessages.length;
    if (nextCount <= windowThreshold) {
      setRenderStartIndex(0);
      return;
    }
    setRenderStartIndex(Math.max(0, nextCount - windowSize));
  }, [sessionId, visibleMessages.length, windowSize, windowThreshold]);

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
    if (smooth) {
      element.scrollTo({ top: element.scrollHeight, behavior: 'smooth' });
      return;
    }
    element.scrollTop = element.scrollHeight;
  }, []);

  const scheduleAutoScrollToBottom = useCallback(() => {
    if (autoScrollRafRef.current !== null) {
      return;
    }
    autoScrollRafRef.current = requestAnimationFrame(() => {
      autoScrollRafRef.current = null;
      scrollToBottom(false);
    });
  }, [scrollToBottom]);

  const scheduleStreamEndBottomLock = useCallback((frames = 8) => {
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
  }, [cancelPendingStreamEndScroll, scrollToBottom]);

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
        if (isStreaming) {
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
  }, [sessionId, isStreaming, renderedMessages.length]);

  useEffect(() => {
    if (isStreaming && autoScroll) {
      scheduleAutoScrollToBottom();
    }
  }, [visibleMessages, isStreaming, autoScroll, scheduleAutoScrollToBottom]);

  useEffect(() => {
    const wasStreaming = prevIsStreamingRef.current;
    prevIsStreamingRef.current = isStreaming;
    if (!wasStreaming || isStreaming) {
      return;
    }

    if (!(autoScroll || isAtBottom)) {
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
  ]);

  useEffect(() => {
    if (isStreaming && isAtBottom) {
      setAutoScroll((prev) => (prev ? prev : true));
      return;
    }
    if (!isStreaming) {
      setAutoScroll((prev) => (prev ? false : prev));
    }
  }, [isStreaming, isAtBottom]);

  const handleScroll = useCallback(() => {
    if (!shouldWindowMessages) {
      return;
    }
    if (scrollRafRef.current !== null) {
      return;
    }
    scrollRafRef.current = requestAnimationFrame(() => {
      scrollRafRef.current = null;
      const element = scrollRef.current;
      if (!element) {
        return;
      }
      if (
        boundedRenderStartIndex > 0
        && element.scrollTop <= MESSAGE_WINDOW_EXPAND_TOP_OFFSET
      ) {
        expandRenderedWindow();
      }
    });
  }, [boundedRenderStartIndex, expandRenderedWindow, shouldWindowMessages]);

  const handleJumpToBottom = useCallback(() => {
    scrollToBottom(true);
    setAutoScroll(true);
  }, [scrollToBottom]);

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
