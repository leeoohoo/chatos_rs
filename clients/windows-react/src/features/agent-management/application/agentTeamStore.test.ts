import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentTeamDetail, AgentTeamSummary } from "../domain/agentManagement";
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

  it("loads the selected team members and rejects a late previous selection", async () => {
    const other = { ...team(), id: "22222222-2222-4222-8222-222222222222", projectId: "project-2" };
    const late = deferred<AgentTeamDetail>();
    const port = stubPort({
      listTeams: vi.fn(async () => [team(), other]),
      loadTeam: vi.fn(async (_owner, teamId) => teamId === team().id ? late.promise : detail(other)),
    });
    await useAgentTeamStore.getState().activate(port, "owner-1");
    const stale = useAgentTeamStore.getState().selectTeam(port, team().id);
    await useAgentTeamStore.getState().selectTeam(port, other.id);
    late.resolve(detail(team()));
    await stale;
    expect(useAgentTeamStore.getState().selectedTeamId).toBe(other.id);
    expect(useAgentTeamStore.getState().detail?.team.id).toBe(other.id);
  });

  it("invites an Agent and updates both detail and team summary", async () => {
    const invited = detail({ ...team(), memberCount: 1, defaultAgentId: "agent-1" }, [{
      agentId: "agent-1", role: "设计师", responsibility: "负责界面", joinedAtUnixMs: 30,
    }]);
    const port = stubPort({ inviteAgentToTeam: vi.fn(async () => invited) });
    await useAgentTeamStore.getState().activate(port, "owner-1");
    await useAgentTeamStore.getState().selectTeam(port, team().id);
    expect(await useAgentTeamStore.getState().inviteAgent(port, team().id, "agent-1")).toBe(true);
    expect(useAgentTeamStore.getState().detail).toEqual(invited);
    expect(useAgentTeamStore.getState().teams[0]?.memberCount).toBe(1);
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
    loadTeam: vi.fn(async () => detail(team())),
    inviteAgentToTeam: vi.fn(async () => detail(team())),
    ...overrides,
  };
}

function detail(
  summary: AgentTeamSummary,
  members: AgentTeamDetail["members"] = [],
): AgentTeamDetail {
  return { team: summary, members };
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
