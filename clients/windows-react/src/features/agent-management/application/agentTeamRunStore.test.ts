import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { useAgentTeamRunStore } from "./agentTeamRunStore";

describe("agent team run store", () => {
  beforeEach(() => useAgentTeamRunStore.getState().reset());

  it("loads the exact team run history", async () => {
    const port = stubPort();
    await useAgentTeamRunStore.getState().activate(port, "owner-1", TEAM_ID);
    expect(port.loadTeamRuns).toHaveBeenCalledWith("owner-1", TEAM_ID);
    expect(useAgentTeamRunStore.getState().snapshot?.runs[0]?.runId).toBe("run-1");
  });

  it("does not let a late previous team overwrite the selected team", async () => {
    let release!: (value: ReturnType<typeof snapshot>) => void;
    const late = new Promise<ReturnType<typeof snapshot>>((resolve) => { release = resolve; });
    const otherId = "22222222-2222-4222-8222-222222222222";
    const port = stubPort({
      loadTeamRuns: vi.fn(async (_owner, teamId) => teamId === TEAM_ID ? late : snapshot(teamId)),
    });
    const stale = useAgentTeamRunStore.getState().activate(port, "owner-1", TEAM_ID);
    await useAgentTeamRunStore.getState().activate(port, "owner-1", otherId);
    release(snapshot(TEAM_ID));
    await stale;
    expect(useAgentTeamRunStore.getState().snapshot?.teamId).toBe(otherId);
  });
});

const TEAM_ID = "11111111-1111-4111-8111-111111111111";

function stubPort(overrides: Partial<AgentManagementPort> = {}): AgentManagementPort {
  return {
    listProfiles: vi.fn(async () => []), loadEditorCatalog: vi.fn(), loadProfileEditor: vi.fn(),
    saveProfile: vi.fn(), pickAvatar: vi.fn(async () => null), loadAvatarHistory: vi.fn(async () => []),
    discardAvatarReferences: vi.fn(async () => undefined), discardAvatarPresentation: vi.fn(async () => undefined),
    loadTeamRuns: vi.fn(async () => snapshot(TEAM_ID)),
    ...overrides,
  };
}

function snapshot(teamId: string) {
  return {
    teamId,
    runs: [{
      deliveryId: "delivery-1", messageId: "message-1", agentId: "agent-1",
      lane: "manager" as const, triggerKind: "default_agent" as const,
      deliveryStatus: "running" as const, runStatus: "running" as const,
      turnId: "turn-1", runId: "run-1", responseMessageId: null, lastError: null,
      createdAtUnixMs: 10, updatedAtUnixMs: 20,
    }],
  };
}
