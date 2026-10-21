import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriAgentManagementAdapter } from "./TauriAgentManagementAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriAgentManagementAdapter avatar boundary", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("passes only owner and presentation identity to the native picker", async () => {
    const draft = {
      id: `reference_${"a".repeat(32)}`,
      name: "avatar.png",
      mimeType: "image/png",
      byteSize: 1_024,
      previewUrl: "chatos-media://localhost/session/reference/presentation/token",
    };
    vi.mocked(invoke).mockResolvedValueOnce(draft);
    await expect(new TauriAgentManagementAdapter().pickAvatar("owner-1", "presentation-1"))
      .resolves.toEqual(draft);
    expect(invoke).toHaveBeenCalledWith("pick_agent_avatar", {
      input: { ownerUserId: "owner-1", presentationId: "presentation-1" },
    });
  });

  it("rejects unsafe picker projections and forwards scoped cleanup", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      id: `reference_${"a".repeat(32)}`,
      name: "avatar.png",
      mimeType: "image/png",
      byteSize: 1_024,
      previewUrl: "file:///private/avatar.png",
    });
    const adapter = new TauriAgentManagementAdapter();
    await expect(adapter.pickAvatar("owner-1", "presentation-1")).rejects.toThrow();

    vi.mocked(invoke).mockResolvedValue(undefined);
    await adapter.discardAvatarReferences("owner-1", "presentation-1", ["reference-1"]);
    await adapter.discardAvatarPresentation("owner-1", "presentation-1");
    expect(invoke).toHaveBeenNthCalledWith(2, "discard_media_studio_reference_images", {
      input: {
        ownerUserId: "owner-1",
        presentationId: "presentation-1",
        referenceIds: ["reference-1"],
      },
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "discard_media_studio_presentation", {
      input: { ownerUserId: "owner-1", presentationId: "presentation-1" },
    });
  });

  it("flattens only controlled image history previews for the avatar picker", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      items: [{
        id: "11111111-1111-4111-8111-111111111111",
        prompt: "portrait",
        modelName: "image-model",
        createdAtUnixMs: 100,
        images: [{
          id: "asset-1",
          mimeType: "image/png",
          contentToken: "b".repeat(32),
          revisedPrompt: null,
          previewUrl: "chatos-media://localhost/22222222-2222-4222-8222-222222222222/11111111-1111-4111-8111-111111111111/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        }],
      }],
      nextCursor: null,
      unreadableCount: 0,
    });
    await expect(new TauriAgentManagementAdapter().loadAvatarHistory("owner-1")).resolves.toEqual([{
      recordId: "11111111-1111-4111-8111-111111111111",
      contentToken: "b".repeat(32),
      previewUrl: "chatos-media://localhost/22222222-2222-4222-8222-222222222222/11111111-1111-4111-8111-111111111111/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      createdAtUnixMs: 100,
    }]);
    expect(invoke).toHaveBeenCalledWith("load_media_studio_image_history", {
      ownerUserId: "owner-1",
      after: null,
      limit: 40,
    });
  });

  it("uses account-scoped native commands for Skill mutations", async () => {
    const item = {
      kind: "profession",
      key: "skill-0",
      label: "名称",
      labelEn: "Name",
      description: "说明",
      descriptionEn: "Description",
      categoryKey: "general",
      categoryLabel: "通用",
      categoryLabelEn: "General",
      markdown: "# 正文",
      markdownEn: "# Body",
      customized: false,
      ...skillProjection("profession", "skill-0"),
    };
    vi.mocked(invoke).mockResolvedValueOnce(Array.from({ length: 60 }, (_, index) => {
      const kind = index < 33 ? "profession" as const : "projectType" as const;
      const key = `skill-${index}`;
      return { ...item, kind, key, ...skillProjection(kind, key) };
    }));
    const adapter = new TauriAgentManagementAdapter();
    await adapter.loadSkillCatalog("owner-1");
    expect(invoke).toHaveBeenLastCalledWith("load_agent_skill_catalog", { ownerUserId: "owner-1" });

    const input = {
      kind: "profession" as const,
      key: "skill-0",
      label: "新名称",
      labelEn: "New name",
      description: "新说明",
      descriptionEn: "New description",
      markdown: "# 新正文",
      markdownEn: "# New body",
    };
    vi.mocked(invoke).mockResolvedValueOnce({ ...item, ...input, customized: true });
    await adapter.saveSkillOverride("owner-1", input);
    expect(invoke).toHaveBeenLastCalledWith("save_agent_skill_override", { ownerUserId: "owner-1", input });

    vi.mocked(invoke).mockResolvedValueOnce(item);
    await adapter.resetSkillOverride("owner-1", "profession", "skill-0");
    expect(invoke).toHaveBeenLastCalledWith("reset_agent_skill_override", {
      ownerUserId: "owner-1", kind: "profession", key: "skill-0",
    });
  });
});

function skillProjection(kind: "profession" | "projectType", key: string) {
  const resources = ["workflow", "deliverables", "quality", "collaboration", "worked-examples-and-counterexamples"].map((name) => ({
    relativePath: `references/${name}.md`, title: name, summary: `${name} summary`, markdown: "body", sizeBytes: 4,
  }));
  return {
    skillRef: `AS-${kind === "projectType" ? "project_type" : "profession"}-${key}-aaaaaaaaaaaa`,
    skillRefEn: `AS-${kind === "projectType" ? "project_type" : "profession"}-${key}-bbbbbbbbbbbb`,
    instructionsSha256: "a".repeat(64),
    instructionsSha256En: "b".repeat(64),
    resources,
    resourcesEn: resources,
  };
}
