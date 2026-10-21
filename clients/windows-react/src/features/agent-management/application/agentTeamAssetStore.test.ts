import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentTeamAsset } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { useAgentTeamAssetStore } from "./agentTeamAssetStore";

describe("agent team asset store", () => {
  beforeEach(() => useAgentTeamAssetStore.getState().reset());

  it("loads, saves and archives revisioned team assets", async () => {
    const port = stubPort();
    await useAgentTeamAssetStore.getState().activate(port, "owner-1", TEAM_ID);
    expect(port.listTeamAssets).toHaveBeenCalledWith("owner-1", TEAM_ID);
    expect(useAgentTeamAssetStore.getState().assets[0]?.title).toBe("项目概览");

    expect(await useAgentTeamAssetStore.getState().save(port, {
      assetId: "asset-1", category: "current_progress", title: "当前进度",
      markdown: "第二版", expectedRevision: 1,
    })).toBe(true);
    expect(useAgentTeamAssetStore.getState().assets[0]?.revision).toBe(2);

    const updated = useAgentTeamAssetStore.getState().assets[0]!;
    expect(await useAgentTeamAssetStore.getState().archive(port, updated)).toBe(true);
    expect(useAgentTeamAssetStore.getState().assets).toEqual([]);
  });

  it("does not let a late previous team overwrite the selected team", async () => {
    let release!: (value: AgentTeamAsset[]) => void;
    const late = new Promise<AgentTeamAsset[]>((resolve) => { release = resolve; });
    const otherId = "22222222-2222-4222-8222-222222222222";
    const port = stubPort({
      listTeamAssets: vi.fn(async (_owner, teamId) => teamId === TEAM_ID ? late : [asset(teamId)]),
    });
    const stale = useAgentTeamAssetStore.getState().activate(port, "owner-1", TEAM_ID);
    await useAgentTeamAssetStore.getState().activate(port, "owner-1", otherId);
    release([asset(TEAM_ID)]);
    await stale;
    expect(useAgentTeamAssetStore.getState().assets[0]?.teamId).toBe(otherId);
  });
});

const TEAM_ID = "11111111-1111-4111-8111-111111111111";

function stubPort(overrides: Partial<AgentManagementPort> = {}): AgentManagementPort {
  return {
    listProfiles: vi.fn(async () => []), loadEditorCatalog: vi.fn(), loadProfileEditor: vi.fn(),
    saveProfile: vi.fn(), pickAvatar: vi.fn(async () => null), loadAvatarHistory: vi.fn(async () => []),
    discardAvatarReferences: vi.fn(async () => undefined), discardAvatarPresentation: vi.fn(async () => undefined),
    listTeamAssets: vi.fn(async () => [asset(TEAM_ID)]),
    saveTeamAsset: vi.fn(async (_owner, teamId, input) => ({
      ...asset(teamId), id: input.assetId ?? "asset-new", category: input.category,
      title: input.title, markdown: input.markdown, revision: (input.expectedRevision ?? 0) + 1,
    })),
    archiveTeamAsset: vi.fn(async (_owner, teamId, assetId, expectedRevision) => ({
      ...asset(teamId), id: assetId, revision: expectedRevision + 1, status: "archived" as const,
    })),
    listTeamAssetRevisions: vi.fn(async () => []),
    ...overrides,
  };
}

function asset(teamId: string): AgentTeamAsset {
  return {
    id: "asset-1", teamId, category: "overview", title: "项目概览", markdown: "第一版",
    revision: 1, status: "active", createdByAgentId: null, updatedByAgentId: null,
    createdAtUnixMs: 10, updatedAtUnixMs: 20,
  };
}
