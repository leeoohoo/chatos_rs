import { beforeEach, describe, expect, it, vi } from "vitest";
import type { HostLifecyclePort } from "../ports/HostLifecyclePort";
import type { HostLifecycleSnapshot } from "../domain/hostLifecycle";
import { useHostLifecycleStore } from "./hostLifecycleStore";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((completion) => {
    resolve = completion;
  });
  return { promise, resolve };
}

function port(
  snapshot: Promise<HostLifecycleSnapshot>,
  overrides: Partial<HostLifecyclePort> = {},
): HostLifecyclePort {
  return {
    snapshot: () => snapshot,
    diagnostics: async () => ({ capacity: 64, entries: [] }),
    start: async () => { throw new Error("unused"); },
    stop: async () => { throw new Error("unused"); },
    ...overrides,
  };
}

const unusedSnapshot = Promise.resolve<HostLifecycleSnapshot>({
  phase: "stopped",
  ready: false,
  protocolVersion: 46,
  detail: "unused",
  retryable: false,
});

describe("hostLifecycleStore", () => {
  beforeEach(() => {
    useHostLifecycleStore.setState({
      requestId: 0,
      diagnostics: [],
      ownerUserId: null,
      snapshot: {
        phase: "stopped",
        ready: false,
        protocolVersion: 46,
        detail: "stopped",
        retryable: false,
        label: "本地服务未启动",
      },
    });
  });

  it("rejects a stale snapshot that finishes after a newer refresh", async () => {
    const first = deferred<HostLifecycleSnapshot>();
    const firstRefresh = useHostLifecycleStore.getState().refresh(port(first.promise));
    const secondRefresh = useHostLifecycleStore.getState().refresh(port(Promise.resolve({
      phase: "ready",
      ready: true,
      protocolVersion: 46,
      detail: "ready",
      retryable: false,
    })));

    await secondRefresh;
    first.resolve({
      phase: "degraded",
      ready: false,
      protocolVersion: 46,
      detail: "stale failure",
      retryable: true,
    });
    await firstRefresh;

    expect(useHostLifecycleStore.getState().snapshot.phase).toBe("ready");
  });

  it("publishes launching before accepting the ready handshake", async () => {
    const start = deferred<HostLifecycleSnapshot>();
    const startRequest = useHostLifecycleStore.getState().start(port(
      unusedSnapshot,
      { start: () => start.promise },
    ), " owner-1 ");

    expect(useHostLifecycleStore.getState().snapshot.phase).toBe("launching");
    expect(useHostLifecycleStore.getState().ownerUserId).toBe("owner-1");

    start.resolve({
      phase: "ready",
      ready: true,
      protocolVersion: 46,
      detail: "ready",
      retryable: false,
    });
    await startRequest;

    expect(useHostLifecycleStore.getState().snapshot.phase).toBe("ready");
  });

  it("rejects an old owner handshake after a new owner starts", async () => {
    const first = deferred<HostLifecycleSnapshot>();
    const second = deferred<HostLifecycleSnapshot>();
    const firstStart = useHostLifecycleStore.getState().start(port(
      unusedSnapshot,
      { start: () => first.promise },
    ), "owner-1");
    const secondStart = useHostLifecycleStore.getState().start(port(
      unusedSnapshot,
      { start: () => second.promise },
    ), "owner-2");

    first.resolve({
      phase: "ready",
      ready: true,
      protocolVersion: 46,
      detail: "stale ready",
      retryable: false,
    });
    second.resolve({
      phase: "degraded",
      ready: false,
      protocolVersion: 46,
      detail: "current failure",
      retryable: true,
    });
    await Promise.all([firstStart, secondStart]);

    expect(useHostLifecycleStore.getState().ownerUserId).toBe("owner-2");
    expect(useHostLifecycleStore.getState().snapshot.detail).toBe("current failure");
  });

  it("retries a failed start with the retained owner", async () => {
    const start = vi
      .fn<HostLifecyclePort["start"]>()
      .mockRejectedValueOnce(new Error("handshake failed"))
      .mockResolvedValueOnce({
        phase: "ready",
        ready: true,
        protocolVersion: 46,
        detail: "ready",
        retryable: false,
      });
    const lifecycle = port(unusedSnapshot, { start });

    await useHostLifecycleStore.getState().start(lifecycle, "owner-1");
    expect(useHostLifecycleStore.getState().snapshot.phase).toBe("degraded");
    await useHostLifecycleStore.getState().retry(lifecycle);

    expect(start).toHaveBeenNthCalledWith(2, "owner-1");
    expect(useHostLifecycleStore.getState().snapshot.phase).toBe("ready");
  });

  it("invalidates an in-flight owner when authentication becomes unavailable", async () => {
    const first = deferred<HostLifecycleSnapshot>();
    const firstStart = useHostLifecycleStore.getState().start(port(
      unusedSnapshot,
      { start: () => first.promise },
    ), "owner-1");

    await useHostLifecycleStore.getState().start(port(unusedSnapshot), "   ");
    first.resolve({
      phase: "ready",
      ready: true,
      protocolVersion: 46,
      detail: "stale ready",
      retryable: false,
    });
    await firstStart;

    expect(useHostLifecycleStore.getState().ownerUserId).toBeNull();
    expect(useHostLifecycleStore.getState().snapshot.phase).toBe("degraded");
    expect(useHostLifecycleStore.getState().snapshot.detail).toContain("登录账号无效");
  });
});
