import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentProfileSummary } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { useAgentManagementStore } from "./agentManagementStore";

describe("agent management store", () => {
  beforeEach(() => useAgentManagementStore.getState().reset());

  it("loads active profiles for the current owner", async () => {
    const port = stubPort();
    await useAgentManagementStore.getState().activate(port, " owner-1 ");
    expect(port.listProfiles).toHaveBeenCalledWith("owner-1");
    expect(useAgentManagementStore.getState().profiles.map((profile) => profile.id)).toEqual(["agent-1"]);
  });

  it("keeps the last trusted profiles when refresh fails", async () => {
    const port = stubPort(vi.fn()
      .mockResolvedValueOnce([fixture()])
      .mockRejectedValueOnce(new Error("database busy")));
    await useAgentManagementStore.getState().activate(port, "owner-1");
    await useAgentManagementStore.getState().refresh(port);
    expect(useAgentManagementStore.getState().profiles).toEqual([fixture()]);
    expect(useAgentManagementStore.getState().error).toBe("database busy");
  });

  it("rejects late results after owner replacement", async () => {
    const late = deferred<AgentProfileSummary[]>();
    const next = { ...fixture(), id: "agent-b" };
    const port = stubPort(vi.fn(async (ownerUserId: string) => (
      ownerUserId === "owner-a" ? late.promise : [next]
    )));
    const stale = useAgentManagementStore.getState().activate(port, "owner-a");
    await useAgentManagementStore.getState().activate(port, "owner-b");
    late.resolve([fixture()]);
    await stale;
    expect(useAgentManagementStore.getState().ownerUserId).toBe("owner-b");
    expect(useAgentManagementStore.getState().profiles).toEqual([next]);
  });

  it("rejects late results after reset", async () => {
    const late = deferred<AgentProfileSummary[]>();
    const port = stubPort(vi.fn(async () => late.promise));
    const load = useAgentManagementStore.getState().activate(port, "owner-1");
    useAgentManagementStore.getState().reset();
    late.resolve([fixture()]);
    await load;
    expect(useAgentManagementStore.getState().profiles).toEqual([]);
  });
});

function stubPort(
  listProfiles: AgentManagementPort["listProfiles"] = vi.fn(async () => [fixture()]),
): AgentManagementPort {
  return { listProfiles };
}

function fixture(): AgentProfileSummary {
  return {
    id: "agent-1",
    name: "设计师",
    description: "负责界面体验",
    modelConfigId: "model-1",
    thinkingLevel: "high",
    professionKey: "general_member",
    heartbeatEnabled: true,
    heartbeatIntervalSeconds: 900,
    lastHeartbeatAtUnixMs: 100,
    nextHeartbeatAtUnixMs: 200,
    canManageStaff: false,
    canAccessLocalProjects: true,
    hasAvatar: false,
    createdAtUnixMs: 10,
    updatedAtUnixMs: 20,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
