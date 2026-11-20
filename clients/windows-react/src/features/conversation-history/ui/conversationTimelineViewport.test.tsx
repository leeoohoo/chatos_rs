import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  isTimelinePinnedToBottom,
  restoredScrollTopAfterPrepend,
  useConversationTimelineViewport,
  type TimelineMessageMarkerSource,
} from "./conversationTimelineViewport";

describe("conversation timeline viewport", () => {
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("detects the macOS 28-pixel bottom boundary", () => {
    expect(isTimelinePinnedToBottom({ clientHeight: 300, scrollHeight: 1_000, scrollTop: 672 })).toBe(true);
    expect(isTimelinePinnedToBottom({ clientHeight: 300, scrollHeight: 1_000, scrollTop: 671 })).toBe(false);
  });

  it("restores the same reading position after older messages are prepended", () => {
    expect(restoredScrollTopAfterPrepend({ scrollHeight: 1_000, scrollTop: 120 }, 1_440)).toBe(560);
    expect(restoredScrollTopAfterPrepend({ scrollHeight: 1_000, scrollTop: 0 }, 800)).toBe(0);
  });

  it("starts at the latest message and exposes newer activity without stealing a reader's viewport", () => {
    const loadOlder = vi.fn();
    const { rerender } = render(
      <ViewportHarness
        clientHeight={300}
        isLoadingOlder={false}
        loadOlder={loadOlder}
        messages={[message(1), message(2)]}
        scrollHeight={1_000}
      />,
    );
    const viewport = screen.getByTestId("viewport");
    expect(viewport.scrollTop).toBe(700);

    viewport.scrollTop = 300;
    fireEvent.scroll(viewport);
    rerender(
      <ViewportHarness
        clientHeight={300}
        isLoadingOlder={false}
        loadOlder={loadOlder}
        messages={[message(1), message(2), message(3)]}
        scrollHeight={1_180}
      />,
    );

    expect(viewport.scrollTop).toBe(300);
    expect(screen.getByRole("button", { name: "1 条新任务动态" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "1 条新任务动态" }));
    expect(viewport.scrollTop).toBe(880);
    expect(screen.queryByRole("button", { name: "1 条新任务动态" })).not.toBeInTheDocument();
  });

  it("counts an updated latest message while the reader is away from the bottom", () => {
    const loadOlder = vi.fn();
    const { rerender } = render(
      <ViewportHarness
        clientHeight={300}
        isLoadingOlder={false}
        loadOlder={loadOlder}
        messages={[message(1), message(2, "partial")]}
        scrollHeight={1_000}
      />,
    );
    const viewport = screen.getByTestId("viewport");
    viewport.scrollTop = 200;
    fireEvent.scroll(viewport);

    rerender(
      <ViewportHarness
        clientHeight={300}
        isLoadingOlder={false}
        loadOlder={loadOlder}
        messages={[message(1), message(2, "finished")]}
        scrollHeight={1_100}
      />,
    );

    expect(viewport.scrollTop).toBe(200);
    expect(screen.getByRole("button", { name: "1 条新任务动态" })).toBeVisible();
  });

  it("keeps the first visible content stable across an older-page request", () => {
    const loadOlder = vi.fn();
    const { rerender } = render(
      <ViewportHarness
        clientHeight={300}
        isLoadingOlder={false}
        loadOlder={loadOlder}
        messages={[message(3), message(4)]}
        scrollHeight={1_000}
      />,
    );
    const viewport = screen.getByTestId("viewport");
    viewport.scrollTop = 120;
    fireEvent.scroll(viewport);
    fireEvent.click(screen.getByRole("button", { name: "加载更早消息" }));
    expect(loadOlder).toHaveBeenCalledTimes(1);

    rerender(
      <ViewportHarness
        clientHeight={300}
        isLoadingOlder
        loadOlder={loadOlder}
        messages={[message(3), message(4)]}
        scrollHeight={1_000}
      />,
    );
    rerender(
      <ViewportHarness
        clientHeight={300}
        isLoadingOlder={false}
        loadOlder={loadOlder}
        messages={[message(1), message(2), message(3), message(4)]}
        scrollHeight={1_440}
      />,
    );

    expect(viewport.scrollTop).toBe(560);
    expect(screen.queryByRole("button", { name: /新任务动态/ })).not.toBeInTheDocument();
  });

  it("releases the frozen older-page anchor after a failed request so retry can proceed", () => {
    const loadOlder = vi.fn();
    const { rerender } = render(
      <ViewportHarness
        clientHeight={300}
        isLoadingOlder={false}
        loadOlder={loadOlder}
        messages={[message(3), message(4)]}
        scrollHeight={1_000}
      />,
    );
    const viewport = screen.getByTestId("viewport");
    viewport.scrollTop = 100;
    fireEvent.scroll(viewport);
    fireEvent.click(screen.getByRole("button", { name: "加载更早消息" }));

    rerender(
      <ViewportHarness
        clientHeight={300}
        isLoadingOlder
        loadOlder={loadOlder}
        messages={[message(3), message(4)]}
        scrollHeight={1_000}
      />,
    );
    rerender(
      <ViewportHarness
        clientHeight={300}
        isLoadingOlder={false}
        loadOlder={loadOlder}
        messages={[message(3), message(4)]}
        scrollHeight={1_020}
      />,
    );

    expect(viewport.scrollTop).toBe(120);
    viewport.scrollTop = 90;
    fireEvent.click(screen.getByRole("button", { name: "加载更早消息" }));
    expect(loadOlder).toHaveBeenCalledTimes(2);
  });

  it("starts observing deferred timeline content once the ready view mounts", () => {
    const observe = vi.fn();
    let notifyResize: ResizeObserverCallback = () => undefined;
    class ResizeObserverStub implements ResizeObserver {
      constructor(callback: ResizeObserverCallback) {
        notifyResize = callback;
      }
      disconnect = vi.fn();
      observe = observe;
      unobserve = vi.fn();
    }
    vi.stubGlobal("ResizeObserver", ResizeObserverStub);
    const loadOlder = vi.fn();
    const { rerender } = render(
      <ViewportHarness
        active={false}
        clientHeight={300}
        isLoadingOlder={false}
        loadOlder={loadOlder}
        messages={[]}
        scrollHeight={0}
      />,
    );
    expect(observe).not.toHaveBeenCalled();

    rerender(
      <ViewportHarness
        active
        clientHeight={300}
        isLoadingOlder={false}
        loadOlder={loadOlder}
        messages={[message(1)]}
        scrollHeight={1_000}
      />,
    );
    const viewport = screen.getByTestId("viewport");
    expect(observe).toHaveBeenCalledTimes(1);
    expect(viewport.scrollTop).toBe(700);

    Object.defineProperty(viewport, "scrollHeight", { configurable: true, value: 1_200 });
    notifyResize([], {} as ResizeObserver);
    expect(viewport.scrollTop).toBe(900);
  });
});

function ViewportHarness({
  active = true,
  clientHeight,
  isLoadingOlder,
  loadOlder,
  messages,
  scrollHeight,
}: {
  active?: boolean;
  clientHeight: number;
  isLoadingOlder: boolean;
  loadOlder(): void;
  messages: TimelineMessageMarkerSource[];
  scrollHeight: number;
}) {
  const [conversationKey] = useState("owner-1:conversation-1");
  const viewport = useConversationTimelineViewport({
    active,
    conversationKey,
    isLoadingOlder,
    messages,
    onLoadOlder: loadOlder,
  });
  return (
    <div>
      <div
        data-testid="viewport"
        onScroll={viewport.onScroll}
        ref={(element) => {
          viewport.scrollRef.current = element;
          if (!element) return;
          Object.defineProperty(element, "clientHeight", { configurable: true, value: clientHeight });
          Object.defineProperty(element, "scrollHeight", { configurable: true, value: scrollHeight });
        }}
      >
        <div ref={viewport.contentRef} />
      </div>
      <button onClick={viewport.requestOlder} type="button">加载更早消息</button>
      {viewport.unreadNewerCount > 0 ? (
        <button aria-label={`${viewport.unreadNewerCount} 条新任务动态`} onClick={viewport.jumpToLatest} type="button">
          newer
        </button>
      ) : null}
    </div>
  );
}

function message(ordinal: number, content = `message-${ordinal}`): TimelineMessageMarkerSource {
  return {
    message_id: `message-${ordinal}`,
    ordinal,
    content,
    metadata: {},
  };
}
