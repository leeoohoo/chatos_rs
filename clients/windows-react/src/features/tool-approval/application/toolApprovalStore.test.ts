import { beforeEach, describe, expect, it, vi } from "vitest";
import { sampleToolApproval } from "../../../test/toolApprovalFixture";
import type {
  ToolApproval,
  ToolApprovalResolution,
} from "../domain/toolApproval";
import type { ToolApprovalPort } from "../ports/ToolApprovalPort";
import { useToolApprovalStore } from "./toolApprovalStore";

describe("tool approval store", () => {
  beforeEach(() => useToolApprovalStore.getState().reset());

  it("keeps the last trusted queue when fallback polling fails", async () => {
    const approval = sampleToolApproval();
    const port = stubPort({
      listPending: vi.fn()
        .mockResolvedValueOnce([approval])
        .mockRejectedValueOnce(new Error("Host restarting")),
    });
    await useToolApprovalStore.getState().activate(port, "user-1");
    await useToolApprovalStore.getState().refresh(port);
    expect(useToolApprovalStore.getState().approvals).toEqual([approval]);
    expect(useToolApprovalStore.getState().loadError).toBe("Host restarting");
  });

  it("rejects a late refresh after the owner changes", async () => {
    const first = deferred<ToolApproval[]>();
    const port = stubPort({
      listPending: vi.fn(async (owner) => (
        owner === "user-a" ? first.promise : [sampleToolApproval({ invocationId: "b" })]
      )),
    });
    const stale = useToolApprovalStore.getState().activate(port, "user-a");
    await useToolApprovalStore.getState().activate(port, "user-b");
    first.resolve([sampleToolApproval({ invocationId: "a" })]);
    await stale;
    expect(useToolApprovalStore.getState().ownerUserId).toBe("user-b");
    expect(useToolApprovalStore.getState().approvals.map((item) => item.invocationId)).toEqual(["b"]);
  });

  it("allows independent decisions and rejects duplicate clicks", async () => {
    const a = sampleToolApproval({ invocationId: "a" });
    const b = sampleToolApproval({ invocationId: "b" });
    let pending = [a, b];
    const first = deferred<ToolApprovalResolution>();
    const second = deferred<ToolApprovalResolution>();
    const port = stubPort({
      listPending: vi.fn(async () => pending),
      decide: vi.fn(async (request) => {
        const resolution = await (request.invocationId === "a" ? first.promise : second.promise);
        pending = pending.filter((item) => item.invocationId !== request.invocationId);
        return resolution;
      }),
    });
    await useToolApprovalStore.getState().activate(port, "user-1");
    const actionA = useToolApprovalStore.getState().decide(port, a, "accept");
    expect(await useToolApprovalStore.getState().decide(port, a, "decline")).toBe(false);
    const actionB = useToolApprovalStore.getState().decide(port, b, "decline");
    expect(Object.keys(useToolApprovalStore.getState().actionIds).sort()).toEqual(["a", "b"]);

    first.resolve(resolution(a, "accept"));
    expect(await actionA).toBe(true);
    second.resolve(resolution(b, "decline"));
    expect(await actionB).toBe(true);
    expect(useToolApprovalStore.getState().approvals).toEqual([]);
    expect(useToolApprovalStore.getState().actionIds).toEqual({});
  });

  it("does not report an older reconciliation as failed when newer truth wins", async () => {
    const a = sampleToolApproval({ invocationId: "a" });
    const b = sampleToolApproval({ invocationId: "b" });
    const decisionA = deferred<ToolApprovalResolution>();
    const decisionB = deferred<ToolApprovalResolution>();
    const staleTruth = deferred<ToolApproval[]>();
    const latestTruth = deferred<ToolApproval[]>();
    const listPending = vi.fn()
      .mockResolvedValueOnce([a, b])
      .mockImplementationOnce(async () => staleTruth.promise)
      .mockImplementationOnce(async () => latestTruth.promise);
    const port = stubPort({
      listPending,
      decide: vi.fn(async (request) => (
        request.invocationId === "a" ? decisionA.promise : decisionB.promise
      )),
    });
    await useToolApprovalStore.getState().activate(port, "user-1");
    const actionA = useToolApprovalStore.getState().decide(port, a, "accept");
    const actionB = useToolApprovalStore.getState().decide(port, b, "decline");
    decisionA.resolve(resolution(a, "accept"));
    await waitForCalls(listPending, 2);
    decisionB.resolve(resolution(b, "decline"));
    await waitForCalls(listPending, 3);
    latestTruth.resolve([]);
    expect(await actionB).toBe(true);
    staleTruth.resolve([b]);
    expect(await actionA).toBe(true);
    expect(useToolApprovalStore.getState().approvals).toEqual([]);
    expect(useToolApprovalStore.getState().loadError).toBeNull();
  });

  it("keeps a per-approval failure and rereads Host truth", async () => {
    const approval = sampleToolApproval();
    const port = stubPort({
      listPending: vi.fn()
        .mockResolvedValueOnce([approval])
        .mockResolvedValueOnce([approval]),
      decide: vi.fn(async () => { throw new Error("version conflict"); }),
    });
    await useToolApprovalStore.getState().activate(port, "user-1");
    expect(await useToolApprovalStore.getState().decide(port, approval, "accept")).toBe(false);
    expect(useToolApprovalStore.getState().approvalErrors[approval.invocationId]).toBe("version conflict");
    expect(useToolApprovalStore.getState().approvals).toEqual([approval]);
    expect(port.listPending).toHaveBeenCalledTimes(2);
  });

  it("rejects a late decision result after Host becomes unavailable", async () => {
    const approval = sampleToolApproval();
    const late = deferred<ToolApprovalResolution>();
    const port = stubPort({
      listPending: vi.fn(async () => [approval]),
      decide: vi.fn(async () => late.promise),
    });
    await useToolApprovalStore.getState().activate(port, "user-1");
    const action = useToolApprovalStore.getState().decide(port, approval, "accept");
    useToolApprovalStore.getState().suspend("user-1");
    late.resolve(resolution(approval, "accept"));
    expect(await action).toBe(false);
    expect(useToolApprovalStore.getState().approvals).toEqual([approval]);
    expect(useToolApprovalStore.getState().actionIds).toEqual({});
  });
});

function resolution(
  approval: ToolApproval,
  decision: ToolApprovalResolution["decision"],
): ToolApprovalResolution {
  return {
    invocationId: approval.invocationId,
    invocationVersion: approval.invocationVersion + 1,
    decision,
  };
}

function stubPort(overrides: Partial<ToolApprovalPort> = {}): ToolApprovalPort {
  return {
    listPending: vi.fn(async () => []),
    decide: vi.fn(async () => { throw new Error("not implemented"); }),
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}

async function waitForCalls(mock: ReturnType<typeof vi.fn>, count: number) {
  for (let attempt = 0; attempt < 20 && mock.mock.calls.length < count; attempt += 1) {
    await Promise.resolve();
  }
  expect(mock).toHaveBeenCalledTimes(count);
}
