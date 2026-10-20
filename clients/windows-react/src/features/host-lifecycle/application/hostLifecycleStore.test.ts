import { beforeEach, describe, expect, it } from "vitest";
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

function port(snapshot: Promise<HostLifecycleSnapshot>): HostLifecyclePort {
  return {
    snapshot: () => snapshot,
    diagnostics: async () => ({ capacity: 64, entries: [] }),
    start: async () => { throw new Error("unused"); },
    stop: async () => { throw new Error("unused"); },
  };
}

describe("hostLifecycleStore", () => {
  beforeEach(() => {
    useHostLifecycleStore.setState({ requestId: 0, diagnostics: [] });
  });

  it("rejects a stale snapshot that finishes after a newer refresh", async () => {
    const first = deferred<HostLifecycleSnapshot>();
    const firstRefresh = useHostLifecycleStore.getState().refresh(port(first.promise));
    const secondRefresh = useHostLifecycleStore.getState().refresh(port(Promise.resolve({
      phase: "ready",
      ready: true,
      protocolVersion: 44,
      detail: "ready",
      retryable: false,
    })));

    await secondRefresh;
    first.resolve({
      phase: "degraded",
      ready: false,
      protocolVersion: 44,
      detail: "stale failure",
      retryable: true,
    });
    await firstRefresh;

    expect(useHostLifecycleStore.getState().snapshot.phase).toBe("ready");
  });
});
