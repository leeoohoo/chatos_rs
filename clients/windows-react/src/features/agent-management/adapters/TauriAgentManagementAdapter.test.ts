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

  it("uses the dedicated native Agent conversation commands and parameters", async () => {
    const roomId = "11111111-1111-4111-8111-111111111111";
    vi.mocked(invoke).mockResolvedValueOnce({
      roomId,
      agentId: "agent-1",
      conversationId: `agent-direct-${roomId}`,
      title: "界面设计师",
      createdAtUnixMs: 10,
      updatedAtUnixMs: 20,
    });
    const adapter = new TauriAgentManagementAdapter();
    await adapter.prepareConversation("owner-1", "agent-1");
    expect(invoke).toHaveBeenLastCalledWith("prepare_agent_conversation", {
      ownerUserId: "owner-1",
      agentId: "agent-1",
    });

    vi.mocked(invoke).mockResolvedValueOnce({
      roomId,
      agentId: "agent-1",
      conversationId: `agent-direct-${roomId}`,
      conversationVersion: 1,
      hasActiveDelivery: false,
      items: [],
    });
    await adapter.loadConversationActivity("owner-1", roomId);
    expect(invoke).toHaveBeenLastCalledWith("load_agent_conversation_activity", {
      ownerUserId: "owner-1",
      roomId,
    });

    vi.mocked(invoke).mockResolvedValueOnce({
      agentId: "agent-1",
      hasActiveDelivery: false,
      items: [],
    });
    await adapter.loadProfileActivity("owner-1", "agent-1");
    expect(invoke).toHaveBeenLastCalledWith("load_agent_profile_activity", {
      ownerUserId: "owner-1",
      agentId: "agent-1",
    });

    const team = {
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
    vi.mocked(invoke).mockResolvedValueOnce([team]);
    await adapter.listTeams("owner-1");
    expect(invoke).toHaveBeenLastCalledWith("list_agent_teams", { ownerUserId: "owner-1" });

    vi.mocked(invoke).mockResolvedValueOnce(team);
    const createInput = { projectId: "project-1", name: "Alpha Agent 团队", goal: "交付 Alpha" };
    await adapter.createTeam("owner-1", createInput);
    expect(invoke).toHaveBeenLastCalledWith("create_agent_team", {
      input: { ownerUserId: "owner-1", ...createInput },
    });

    const detail = { team, members: [] };
    vi.mocked(invoke).mockResolvedValueOnce(detail);
    await adapter.loadTeam("owner-1", team.id);
    expect(invoke).toHaveBeenLastCalledWith("load_agent_team", {
      ownerUserId: "owner-1",
      teamId: team.id,
    });

    const invitedDetail = {
      team: { ...team, memberCount: 1, defaultAgentId: "agent-1" },
      members: [{ agentId: "agent-1", role: "界面设计师", responsibility: "负责界面", joinedAtUnixMs: 20 }],
    };
    vi.mocked(invoke).mockResolvedValueOnce(invitedDetail);
    await adapter.inviteAgentToTeam("owner-1", team.id, "agent-1");
    expect(invoke).toHaveBeenLastCalledWith("invite_agent_to_team", {
      input: { ownerUserId: "owner-1", teamId: team.id, agentId: "agent-1" },
    });

    vi.mocked(invoke).mockResolvedValueOnce(invitedDetail);
    await adapter.updateTeamMember("owner-1", team.id, "agent-1", {
      role: "前端工程师", responsibility: "负责 Windows UI",
    });
    expect(invoke).toHaveBeenLastCalledWith("update_agent_team_member", {
      input: {
        ownerUserId: "owner-1", teamId: team.id, agentId: "agent-1",
        role: "前端工程师", responsibility: "负责 Windows UI",
      },
    });

    for (const [method, command] of [
      ["setTeamDefaultAgent", "set_agent_team_default_agent"],
      ["setTeamProjectManager", "set_agent_team_project_manager"],
      ["removeAgentFromTeam", "remove_agent_from_team"],
    ] as const) {
      vi.mocked(invoke).mockResolvedValueOnce(invitedDetail);
      await adapter[method]("owner-1", team.id, "agent-1");
      expect(invoke).toHaveBeenLastCalledWith(command, {
        input: { ownerUserId: "owner-1", teamId: team.id, agentId: "agent-1" },
      });
    }

    vi.mocked(invoke).mockResolvedValueOnce({ teamId: team.id, messages: [], deliveries: [] });
    await adapter.loadTeamConversation("owner-1", team.id);
    expect(invoke).toHaveBeenLastCalledWith("load_agent_team_conversation", {
      ownerUserId: "owner-1", teamId: team.id,
    });

    vi.mocked(invoke).mockResolvedValueOnce({
      messageId: "message-1", teamId: team.id, createdAtUnixMs: 30,
      deliveries: [{ deliveryId: "delivery-1", targetAgentId: "agent-1", triggerKind: "default_agent" }],
    });
    await adapter.queueTeamMessage("owner-1", team.id, "开始实现", []);
    expect(invoke).toHaveBeenLastCalledWith("queue_agent_team_message", {
      input: { ownerUserId: "owner-1", teamId: team.id, content: "开始实现", mentionedAgentIds: [] },
    });

    vi.mocked(invoke).mockResolvedValueOnce({
      deliveryId: "delivery-1", teamId: team.id, agentId: "agent-1", turnId: "turn-1", runId: "run-1",
    });
    await adapter.dispatchNextTeamDelivery("owner-1", team.id);
    expect(invoke).toHaveBeenLastCalledWith("dispatch_next_agent_team_delivery", {
      ownerUserId: "owner-1", teamId: team.id,
    });

    vi.mocked(invoke).mockResolvedValueOnce({
      roomId: team.id,
      agentId: "agent-1",
      conversationId: `agent-team-${team.id}-agent-1`,
      conversationVersion: 0,
      hasActiveDelivery: false,
      items: [],
    });
    await adapter.loadTeamMemberActivity("owner-1", team.id, "agent-1");
    expect(invoke).toHaveBeenLastCalledWith("load_agent_team_member_activity", {
      ownerUserId: "owner-1", teamId: team.id, agentId: "agent-1",
    });

    vi.mocked(invoke).mockResolvedValueOnce({
      teamId: team.id,
      messageCount: 1,
      pendingDeliveryCount: 0,
      runningDeliveryCount: 1,
      completedDeliveryCount: 0,
      failedDeliveryCount: 0,
      cancelledDeliveryCount: 0,
      runs: [{
        deliveryId: "delivery-1", messageId: "message-1", agentId: "agent-1",
        lane: "manager", triggerKind: "default_agent", deliveryStatus: "running", runStatus: "running",
        turnId: "turn-1", runId: "run-1", responseMessageId: null, lastError: null,
        createdAtUnixMs: 30, updatedAtUnixMs: 40,
      }],
    });
    await adapter.loadTeamRuns("owner-1", team.id);
    expect(invoke).toHaveBeenLastCalledWith("load_agent_team_runs", {
      ownerUserId: "owner-1", teamId: team.id,
    });

    const asset = {
      id: "asset-1", teamId: team.id, category: "overview", title: "项目概览",
      markdown: "# Alpha", revision: 1, status: "active",
      createdByAgentId: null, updatedByAgentId: null,
      createdAtUnixMs: 40, updatedAtUnixMs: 40,
    };
    vi.mocked(invoke).mockResolvedValueOnce([asset]);
    await adapter.listTeamAssets("owner-1", team.id);
    expect(invoke).toHaveBeenLastCalledWith("list_agent_team_assets", {
      ownerUserId: "owner-1", teamId: team.id,
    });

    vi.mocked(invoke).mockResolvedValueOnce({ ...asset, revision: 2 });
    await adapter.saveTeamAsset("owner-1", team.id, {
      assetId: "asset-1", category: "overview", title: "项目概览",
      markdown: "# Alpha v2", expectedRevision: 1,
    });
    expect(invoke).toHaveBeenLastCalledWith("save_agent_team_asset", { input: {
      ownerUserId: "owner-1", teamId: team.id, assetId: "asset-1",
      category: "overview", title: "项目概览", markdown: "# Alpha v2", expectedRevision: 1,
    } });

    vi.mocked(invoke).mockResolvedValueOnce({ ...asset, revision: 3, status: "archived" });
    await adapter.archiveTeamAsset("owner-1", team.id, "asset-1", 2);
    expect(invoke).toHaveBeenLastCalledWith("archive_agent_team_asset", { input: {
      ownerUserId: "owner-1", teamId: team.id, assetId: "asset-1", expectedRevision: 2,
    } });

    vi.mocked(invoke).mockResolvedValueOnce([{
      assetId: "asset-1", revision: 2, title: "项目概览", markdown: "# Alpha v2",
      editorAgentId: null, createdAtUnixMs: 50,
    }]);
    await adapter.listTeamAssetRevisions("owner-1", team.id, "asset-1");
    expect(invoke).toHaveBeenLastCalledWith("list_agent_team_asset_revisions", {
      ownerUserId: "owner-1", teamId: team.id, assetId: "asset-1",
    });

    const input = {
      ownerUserId: "owner-1",
      roomId,
      guidanceTurnId: null,
      submission: { content: "继续", attachmentIds: ["attachment-1"] },
    };
    vi.mocked(invoke).mockResolvedValueOnce({
      deliveryKind: "newTurn",
      turnId: "turn-1",
      userMessageId: "message-1",
    });
    await adapter.sendConversationDraft(input);
    expect(invoke).toHaveBeenLastCalledWith("send_agent_conversation_draft", { input });
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
