import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AgentProfileEditorCatalog,
  AgentProfileEditorData,
  AgentProfileMutationInput,
  AgentProfileSummary,
} from "../domain/agentManagement";
import { agentProfessionLabels } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { useAgentManagementStore } from "./agentManagementStore";

describe("agent management store", () => {
  beforeEach(() => useAgentManagementStore.getState().reset());

  it("loads active profiles for the current owner", async () => {
    const port = stubPort();
    await useAgentManagementStore.getState().activate(port, " owner-1 ");
    expect(port.listProfiles).toHaveBeenCalledWith("owner-1");
    expect(useAgentManagementStore.getState().profiles.map((item) => item.id)).toEqual(["agent-1"]);
  });

  it("keeps the last trusted profiles when refresh fails", async () => {
    const port = stubPort({
      listProfiles: vi.fn().mockResolvedValueOnce([profile()]).mockRejectedValueOnce(new Error("database busy")),
    });
    await useAgentManagementStore.getState().activate(port, "owner-1");
    await useAgentManagementStore.getState().refresh(port);
    expect(useAgentManagementStore.getState().profiles).toEqual([profile()]);
    expect(useAgentManagementStore.getState().error).toBe("database busy");
  });

  it("rejects late profile results after owner replacement", async () => {
    const late = deferred<AgentProfileSummary[]>();
    const next = { ...profile(), id: "agent-b" };
    const port = stubPort({
      listProfiles: vi.fn(async (ownerUserId: string) => ownerUserId === "owner-a" ? late.promise : [next]),
    });
    const stale = useAgentManagementStore.getState().activate(port, "owner-a");
    await useAgentManagementStore.getState().activate(port, "owner-b");
    late.resolve([profile()]);
    await stale;
    expect(useAgentManagementStore.getState().ownerUserId).toBe("owner-b");
    expect(useAgentManagementStore.getState().profiles).toEqual([next]);
  });

  it("rejects late editor catalog results after owner replacement", async () => {
    const late = deferred<AgentProfileEditorCatalog>();
    const port = stubPort({
      loadEditorCatalog: vi.fn(async (ownerUserId: string) => (
        ownerUserId === "owner-a" ? late.promise : catalog("owner-b prompt")
      )),
    });
    await useAgentManagementStore.getState().activate(port, "owner-a");
    const stale = useAgentManagementStore.getState().loadEditorCatalog(port);
    await useAgentManagementStore.getState().activate(port, "owner-b");
    const current = await useAgentManagementStore.getState().loadEditorCatalog(port);
    late.resolve(catalog("owner-a prompt"));
    expect(await stale).toBeNull();
    expect(current?.defaultRolePrompt).toBe("owner-b prompt");
    expect(useAgentManagementStore.getState().editorCatalog?.defaultRolePrompt).toBe("owner-b prompt");
  });

  it("rejects late profile editor results after reset", async () => {
    const late = deferred<AgentProfileEditorData>();
    const port = stubPort({ loadProfileEditor: vi.fn(async () => late.promise) });
    await useAgentManagementStore.getState().activate(port, "owner-1");
    const load = useAgentManagementStore.getState().loadProfileEditor(port, "agent-1");
    useAgentManagementStore.getState().reset();
    late.resolve(editorData());
    expect(await load).toBeNull();
    expect(useAgentManagementStore.getState().editorProfile).toBeNull();
  });

  it("keeps the trusted card and reports stale revision save failures", async () => {
    const port = stubPort({
      saveProfile: vi.fn(async () => { throw new Error("Agent 状态已经变化，请刷新后重试。"); }),
    });
    await useAgentManagementStore.getState().activate(port, "owner-1");
    expect(await useAgentManagementStore.getState().saveProfile(port, mutation())).toBe(false);
    expect(useAgentManagementStore.getState().profiles).toEqual([profile()]);
    expect(useAgentManagementStore.getState().error).toBe("Agent 状态已经变化，请刷新后重试。");
  });

  it("upserts a saved profile in macOS name order", async () => {
    const saved = { ...profile(), id: "agent-2", name: "分析师" };
    const port = stubPort({ saveProfile: vi.fn(async () => saved) });
    await useAgentManagementStore.getState().activate(port, "owner-1");
    expect(await useAgentManagementStore.getState().saveProfile(port, mutation())).toBe(true);
    expect(useAgentManagementStore.getState().profiles.map((item) => item.id)).toEqual(["agent-2", "agent-1"]);
  });

  it("rejects late results after reset", async () => {
    const late = deferred<AgentProfileSummary[]>();
    const port = stubPort({ listProfiles: vi.fn(async () => late.promise) });
    const load = useAgentManagementStore.getState().activate(port, "owner-1");
    useAgentManagementStore.getState().reset();
    late.resolve([profile()]);
    await load;
    expect(useAgentManagementStore.getState().profiles).toEqual([]);
  });
});

function stubPort(overrides: Partial<AgentManagementPort> = {}): AgentManagementPort {
  return {
    listProfiles: vi.fn(async () => [profile()]),
    loadEditorCatalog: vi.fn(async () => catalog()),
    loadProfileEditor: vi.fn(async () => editorData()),
    saveProfile: vi.fn(async () => profile()),
    pickAvatar: vi.fn(async () => null),
    loadAvatarHistory: vi.fn(async () => []),
    discardAvatarReferences: vi.fn(async () => undefined),
    discardAvatarPresentation: vi.fn(async () => undefined),
    ...overrides,
  };
}

function profile(): AgentProfileSummary {
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
    avatarPreviewUrl: null,
    createdAtUnixMs: 10,
    updatedAtUnixMs: 20,
  };
}

function catalog(defaultRolePrompt = "默认角色 Prompt"): AgentProfileEditorCatalog {
  return {
    models: [{
      id: "model-1", displayName: "主要模型", modelName: "gpt-test", provider: "openai",
      defaultThinkingLevel: "high", supportsReasoning: true, thinkingLevels: ["auto", "high"],
    }],
    professions: Object.entries(agentProfessionLabels).map(([key, label]) => ({
      key, label, description: `${label}职责`, categoryKey: "general", categoryLabel: "通用",
    })) as AgentProfileEditorCatalog["professions"],
    defaultRolePrompt,
  };
}

function editorData(): AgentProfileEditorData {
  return { profile: profile(), rolePrompt: "私有角色 Prompt", heartbeatPrompt: "检查阻塞" };
}

function mutation(): AgentProfileMutationInput {
  return {
    agentId: null, expectedUpdatedAtUnixMs: null, name: "分析师", description: "分析需求",
    rolePrompt: "完成分析任务", modelConfigId: "model-1", thinkingLevel: "high",
    professionKey: "business_analyst", canManageStaff: false, canAccessLocalProjects: true,
    heartbeatEnabled: false, heartbeatIntervalSeconds: 900, heartbeatPrompt: "",
    avatar: { kind: "preserve" },
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
