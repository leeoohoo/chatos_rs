import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentTeamSummary } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { useAgentTeamStore } from "./agentTeamStore";

describe("agent team store", () => {
  beforeEach(() => useAgentTeamStore.getState().reset());

  it("loads account-scoped project teams", async () => {
    const port = stubPort();
    await useAgentTeamStore.getState().activate(port, " owner-1 ");
    expect(port.listTeams).toHaveBeenCalledWith("owner-1");
    expect(useAgentTeamStore.getState().teams).toEqual([team()]);
  });

  it("keeps trusted teams when refresh fails", async () => {
    const port = stubPort({
      listTeams: vi.fn().mockResolvedValueOnce([team()]).mockRejectedValueOnce(new Error("database busy")),
    });
    await useAgentTeamStore.getState().activate(port, "owner-1");
    await useAgentTeamStore.getState().refresh(port);
    expect(useAgentTeamStore.getState().teams).toEqual([team()]);
    expect(useAgentTeamStore.getState().error).toBe("database busy");
  });

  it("rejects a late team list after owner replacement", async () => {
    const late = deferred<AgentTeamSummary[]>();
    const other = { ...team(), id: "22222222-2222-4222-8222-222222222222", projectId: "project-2" };
    const port = stubPort({
      listTeams: vi.fn(async (owner: string) => owner === "owner-a" ? late.promise : [other]),
    });
    const stale = useAgentTeamStore.getState().activate(port, "owner-a");
    await useAgentTeamStore.getState().activate(port, "owner-b");
    late.resolve([team()]);
    await stale;
    expect(useAgentTeamStore.getState().ownerUserId).toBe("owner-b");
    expect(useAgentTeamStore.getState().teams).toEqual([other]);
  });

  it("invalidates an older list and inserts a newly created team", async () => {
    const late = deferred<AgentTeamSummary[]>();
    const port = stubPort({ listTeams: vi.fn(async () => late.promise) });
    const loading = useAgentTeamStore.getState().activate(port, "owner-1");
    const created = await useAgentTeamStore.getState().createTeam(port, {
      projectId: "project-1", name: "Alpha Agent 团队", goal: "交付 Alpha",
    });
    late.resolve([]);
    await loading;
    expect(created).toEqual(team());
    expect(useAgentTeamStore.getState().teams).toEqual([team()]);
  });
});

function stubPort(overrides: Partial<AgentManagementPort> = {}): AgentManagementPort {
  return {
    listProfiles: vi.fn(async () => []),
    loadEditorCatalog: vi.fn(),
    loadProfileEditor: vi.fn(),
    saveProfile: vi.fn(),
    pickAvatar: vi.fn(async () => null),
    loadAvatarHistory: vi.fn(async () => []),
    discardAvatarReferences: vi.fn(async () => undefined),
    discardAvatarPresentation: vi.fn(async () => undefined),
    listTeams: vi.fn(async () => [team()]),
    createTeam: vi.fn(async () => team()),
    ...overrides,
  };
}

function team(): AgentTeamSummary {
  return {
    id: "11111111-1111-4111-8111-111111111111",
    projectId: "project-1",
    name: "Alpha Agent 团队",
    goal: "交付 Alpha",
    defaultAgentId: null,
    projectManagerAgentId: null,
    memberCount: 0,
    createdAtUnixMs: 10,
    updatedAtUnixMs: 20,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
