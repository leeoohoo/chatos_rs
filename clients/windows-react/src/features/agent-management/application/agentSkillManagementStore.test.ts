import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSkillDefinition } from "../domain/agentSkillManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { useAgentSkillManagementStore } from "./agentSkillManagementStore";

describe("agent Skill management store", () => {
  beforeEach(() => useAgentSkillManagementStore.getState().reset());

  it("loads only the active account catalog", async () => {
    const port = stubPort();
    await useAgentSkillManagementStore.getState().activate(port, " owner-1 ");
    expect(port.loadSkillCatalog).toHaveBeenCalledWith("owner-1");
    expect(useAgentSkillManagementStore.getState().items).toEqual([skill()]);
  });

  it("rejects late catalog results after an account replacement", async () => {
    const late = deferred<AgentSkillDefinition[]>();
    const ownerTwo = { ...skill(), label: "账号二职业" };
    const port = stubPort({
      loadSkillCatalog: vi.fn(async (owner) => owner === "owner-1" ? late.promise : [ownerTwo]),
    });
    const stale = useAgentSkillManagementStore.getState().activate(port, "owner-1");
    await useAgentSkillManagementStore.getState().activate(port, "owner-2");
    late.resolve([skill()]);
    await stale;
    expect(useAgentSkillManagementStore.getState().ownerUserId).toBe("owner-2");
    expect(useAgentSkillManagementStore.getState().items).toEqual([ownerTwo]);
  });

  it("replaces only the saved definition", async () => {
    const updated = { ...skill(), label: "新名称", customized: true };
    const port = stubPort({ saveSkillOverride: vi.fn(async () => updated) });
    await useAgentSkillManagementStore.getState().activate(port, "owner-1");
    const saved = await useAgentSkillManagementStore.getState().save(port, {
      kind: "profession", key: "project_manager", label: "新名称", labelEn: "New name",
      description: "说明", descriptionEn: "Description", markdown: "# 正文", markdownEn: "# Body",
    });
    expect(saved).toEqual(updated);
    expect(useAgentSkillManagementStore.getState().items).toEqual([updated]);
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
    loadSkillCatalog: vi.fn(async () => [skill()]),
    saveSkillOverride: vi.fn(async () => skill()),
    resetSkillOverride: vi.fn(async () => skill()),
    ...overrides,
  };
}

function skill(): AgentSkillDefinition {
  return {
    kind: "profession",
    key: "project_manager",
    label: "项目经理",
    labelEn: "Project Manager",
    description: "推进项目",
    descriptionEn: "Deliver projects",
    categoryKey: "management",
    categoryLabel: "管理与领导",
    categoryLabelEn: "Management & Leadership",
    markdown: "# 项目经理",
    markdownEn: "# Project Manager",
    customized: false,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
