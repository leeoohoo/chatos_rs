import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentManagementStore } from "../application/agentManagementStore";
import { useAgentSkillManagementStore } from "../application/agentSkillManagementStore";
import { useAgentTeamConversationStore } from "../application/agentTeamConversationStore";
import { useAgentTeamRunStore } from "../application/agentTeamRunStore";
import { useAgentTeamStore } from "../application/agentTeamStore";
import { useAgentTeamTodoStore } from "../application/agentTeamTodoStore";
import {
  agentProfessionLabels,
  type AgentProfileEditorCatalog,
  type AgentProfileActivitySnapshot,
  type AgentProfileSummary,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import type { RunInspectorPort } from "../../run-inspector/ports/RunInspectorPort";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import { AgentManagementView } from "./AgentManagementView";

vi.mock("../../conversation-history/ui/ConversationHistoryView", () => ({
  ConversationHistoryView: ({ agentBound }: { agentBound?: boolean }) => (
    <div data-testid="agent-conversation-history" data-agent-bound={String(agentBound)} />
  ),
}));

describe("AgentManagementView", () => {
  beforeEach(() => useAgentManagementStore.getState().reset());
  beforeEach(() => useAgentSkillManagementStore.getState().reset());
  beforeEach(() => useAgentTeamStore.getState().reset());
  beforeEach(() => useAgentTeamConversationStore.getState().reset());
  beforeEach(() => useAgentTeamRunStore.getState().reset());
  beforeEach(() => useAgentTeamTodoStore.getState().reset());
  afterEach(cleanup);

  it("renders macOS profile card facts and enables profile editing", async () => {
    renderView();
    expect(await screen.findByText("界面设计师")).toBeVisible();
    expect(screen.getByText("UI 设计师")).toBeVisible();
    expect(await screen.findByText("主要模型")).toBeVisible();
    expect(screen.getByText("每 15 分钟")).toBeVisible();
    expect(screen.getByText("项目与团队")).toBeVisible();
    expect(screen.getByRole("button", { name: "＋ 创建 Agent" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "编辑" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "私聊" })).toBeDisabled();
  });

  it("renders a custom avatar only through the controlled Agent protocol", async () => {
    const avatarProfile = {
      ...profile(),
      hasAvatar: true,
      avatarPreviewUrl: "chatos-agent://localhost/11111111-1111-4111-8111-111111111111/agent-1/20",
    };
    renderView(stubPort({ listProfiles: vi.fn(async () => [avatarProfile]) }));
    await screen.findByText("界面设计师");
    const image = document.querySelector<HTMLImageElement>(".agent-card__avatar");
    expect(image?.src).toBe(avatarProfile.avatarPreviewUrl);
  });

  it("loads, previews, and saves Local Agent Host documents", async () => {
    const artifact = {
      artifactId: "artifact-1",
      name: "交付说明.md",
      mimeType: "text/markdown",
      size: 12,
      sha256: "a".repeat(64),
      status: "stored" as const,
      createdAtUnixMs: Date.UTC(2026, 9, 25, 1, 2),
      updatedAtUnixMs: Date.UTC(2026, 9, 25, 1, 2),
    };
    const port = stubPort({
      listArtifacts: vi.fn(async () => ({ artifacts: [artifact], nextCursor: null })),
      previewArtifact: vi.fn(async () => ({ artifactId: artifact.artifactId, markdown: "# 交付说明\n\n- 已完成" })),
      saveArtifact: vi.fn(async () => true),
    });
    renderView(port);
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: /本地 Agent 文档/ }));
    expect(await screen.findByText("交付说明.md")).toBeVisible();
    expect(port.listArtifacts).toHaveBeenCalledWith("owner-1", 50, null);

    fireEvent.click(screen.getByRole("button", { name: "◎ 预览" }));
    expect(await screen.findByRole("dialog", { name: "交付说明.md 文档预览" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "交付说明" })).toBeVisible();
    expect(port.previewArtifact).toHaveBeenCalledWith(
      "owner-1", artifact.artifactId, artifact.size, artifact.sha256,
    );
    fireEvent.click(screen.getByRole("button", { name: "关闭文档预览" }));

    fireEvent.click(screen.getByRole("button", { name: "另存为 交付说明.md" }));
    expect(await screen.findByRole("status")).toHaveTextContent("“交付说明.md”已保存。");
    expect(port.saveArtifact).toHaveBeenCalledWith(
      "owner-1", artifact.artifactId, artifact.name, artifact.size, artifact.sha256,
    );
  });

  it("refreshes the real Agent document list and keeps the macOS empty state", async () => {
    const listArtifacts = vi.fn(async () => ({ artifacts: [], nextCursor: null }));
    renderView(stubPort({ listArtifacts }));
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: /本地 Agent 文档/ }));
    expect(await screen.findByText("还没有本地 Agent 文档")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "刷新本地 Agent 文档" }));
    await waitFor(() => expect(listArtifacts).toHaveBeenCalledTimes(2));
  });

  it("opens creation with macOS defaults", async () => {
    renderView();
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "＋ 创建 Agent" }));
    expect(await screen.findByRole("dialog", { name: "创建 Agent" })).toBeVisible();
    expect(screen.getByLabelText("角色 Prompt")).toHaveValue("默认角色 Prompt");
    expect(screen.getByLabelText("模型")).toHaveValue("model-1");
    expect(screen.getByLabelText(/职业/)).toHaveValue("general_member");
    expect(screen.getByLabelText(/主动巡检/)).not.toBeChecked();
  });

  it("loads private role and heartbeat prompts when editing", async () => {
    const port = stubPort();
    renderView(port);
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "编辑" }));
    expect(await screen.findByRole("dialog", { name: "编辑 Agent" })).toBeVisible();
    expect(port.loadProfileEditor).toHaveBeenCalledWith("owner-1", "agent-1");
    expect(screen.getByLabelText("角色 Prompt")).toHaveValue("私有角色 Prompt");
    expect(screen.getByLabelText("巡检要求")).toHaveValue("检查阻塞和待推进工作");
  });

  it("sends the native mutation contract and closes after save", async () => {
    const port = stubPort({ listProfiles: vi.fn(async () => []) });
    renderView(port);
    await screen.findByText("还没有 Agent");
    fireEvent.click(screen.getByRole("button", { name: /^创建 Agent$/ }));
    await screen.findByRole("dialog", { name: "创建 Agent" });
    fireEvent.change(screen.getByLabelText("名称"), { target: { value: "  新 Agent  " } });
    fireEvent.change(screen.getByLabelText("说明"), { target: { value: "  处理需求  " } });
    fireEvent.change(screen.getByLabelText(/职业/), { target: { value: "business_analyst" } });
    fireEvent.click(screen.getByLabelText(/允许查看项目并创建团队/));
    fireEvent.click(screen.getByRole("button", { name: "创建" }));
    await waitFor(() => expect(port.saveProfile).toHaveBeenCalledWith("owner-1", expect.objectContaining({
      agentId: null, expectedUpdatedAtUnixMs: null, name: "新 Agent", description: "处理需求",
      rolePrompt: "默认角色 Prompt", modelConfigId: "model-1", professionKey: "business_analyst",
      canAccessLocalProjects: true, heartbeatEnabled: false, heartbeatIntervalSeconds: 900,
      heartbeatPrompt: "",
      avatar: { kind: "preserve" },
    })));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "创建 Agent" })).not.toBeInTheDocument());
  });

  it("stages a native avatar reference without exposing bytes or paths", async () => {
    const port = stubPort({
      pickAvatar: vi.fn(async () => ({
        id: `reference_${"a".repeat(32)}`,
        name: "avatar.png",
        mimeType: "image/png" as const,
        byteSize: 1_024,
        previewUrl: "chatos-media://localhost/session/reference/presentation/token",
      })),
    });
    renderView(port);
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "编辑" }));
    await screen.findByRole("dialog", { name: "编辑 Agent" });
    fireEvent.click(screen.getByRole("button", { name: "从本机选择" }));
    expect(await screen.findByRole("img", { name: "Agent 头像预览" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(port.saveProfile).toHaveBeenCalledWith("owner-1", expect.objectContaining({
      avatar: expect.objectContaining({
        kind: "staged",
        referenceId: `reference_${"a".repeat(32)}`,
      }),
    })));
    const payload = vi.mocked(port.saveProfile).mock.calls[0]?.[1];
    expect(JSON.stringify(payload)).not.toContain("avatar.png");
    expect(JSON.stringify(payload)).not.toContain("chatos-media");
  });

  it("selects an avatar from the safe generated image history projection", async () => {
    const historyAsset = {
      recordId: "11111111-1111-4111-8111-111111111111",
      contentToken: "b".repeat(32),
      previewUrl: "chatos-media://localhost/session/11111111-1111-4111-8111-111111111111/token",
      createdAtUnixMs: 100,
    };
    const port = stubPort({ loadAvatarHistory: vi.fn(async () => [historyAsset]) });
    renderView(port);
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "编辑" }));
    await screen.findByRole("dialog", { name: "编辑 Agent" });
    fireEvent.click(screen.getByRole("button", { name: "从生成记录选择" }));
    fireEvent.click(await screen.findByRole("button", { name: "生成的头像候选" }));
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(port.saveProfile).toHaveBeenCalledWith("owner-1", expect.objectContaining({
      avatar: {
        kind: "history",
        recordId: historyAsset.recordId,
        contentToken: historyAsset.contentToken,
      },
    })));
  });

  it("keeps the editor open when conflict protection rejects save", async () => {
    const port = stubPort({
      saveProfile: vi.fn(async () => { throw new Error("Agent 状态已经变化，请刷新后重试。"); }),
    });
    renderView(port);
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "编辑" }));
    await screen.findByRole("dialog", { name: "编辑 Agent" });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(await screen.findByText("Agent 状态已经变化，请刷新后重试。")).toBeVisible();
    expect(screen.getByRole("dialog", { name: "编辑 Agent" })).toBeVisible();
  });

  it("shows a truthful empty state", async () => {
    renderView(stubPort({ listProfiles: vi.fn(async () => []) }));
    expect(await screen.findByText("还没有 Agent")).toBeVisible();
    expect(screen.getByText(/创建后可以直接私聊/)).toBeVisible();
  });

  it("creates one project-bound Agent team with the macOS default name", async () => {
    const createdTeam = {
      id: "11111111-1111-4111-8111-111111111111",
      projectId: "project-1",
      name: "Alpha Agent 团队",
      goal: "",
      defaultAgentId: null,
      projectManagerAgentId: null,
      memberCount: 0,
      createdAtUnixMs: 10,
      updatedAtUnixMs: 10,
    };
    const port = stubPort({
      listTeams: vi.fn(async () => []),
      createTeam: vi.fn(async () => createdTeam),
    });
    render(
      <AgentManagementView
        ownerUserId="owner-1"
        port={port}
        projects={[{ id: "project-1", title: "Alpha" }]}
      />,
    );
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "创建 Agent 团队" }));
    expect(await screen.findByRole("dialog", { name: "创建 Agent 团队" })).toBeVisible();
    expect(screen.getByLabelText("绑定项目")).toHaveValue("project-1");
    fireEvent.click(screen.getByRole("button", { name: "创建团队" }));

    await waitFor(() => expect(port.createTeam).toHaveBeenCalledWith("owner-1", {
      projectId: "project-1",
      name: "Alpha Agent 团队",
      goal: "",
    }));
    expect(await screen.findByText("Alpha Agent 团队")).toBeVisible();
    const memberRail = await screen.findByRole("region", { name: "Agent 团队成员" });
    expect(within(memberRail).getByText("0")).toBeVisible();
  });

  it("loads team members and invites an existing account Agent", async () => {
    const team = {
      id: "11111111-1111-4111-8111-111111111111",
      projectId: "project-1",
      name: "Alpha Agent 团队",
      goal: "交付 Alpha",
      defaultAgentId: "agent-1",
      projectManagerAgentId: null,
      memberCount: 1,
      createdAtUnixMs: 10,
      updatedAtUnixMs: 20,
    };
    const secondProfile = { ...profile(), id: "agent-2", name: "后端工程师", professionKey: "backend_engineer" as const };
    const initialDetail = {
      team,
      members: [{
        agentId: "agent-1", role: "界面设计师", responsibility: "负责产品界面", joinedAtUnixMs: 20,
      }],
    };
    const invitedDetail = {
      team: { ...team, memberCount: 2, updatedAtUnixMs: 30 },
      members: [...initialDetail.members, {
        agentId: "agent-2", role: "后端工程师", responsibility: "负责后端服务", joinedAtUnixMs: 30,
      }],
    };
    const port = stubPort({
      listProfiles: vi.fn(async () => [profile(), secondProfile]),
      listTeams: vi.fn(async () => [team]),
      loadTeam: vi.fn(async () => initialDetail),
      inviteAgentToTeam: vi.fn(async () => invitedDetail),
    });
    render(
      <AgentManagementView
        ownerUserId="owner-1"
        port={port}
        projects={[{ id: "project-1", title: "Alpha" }]}
      />,
    );
    await screen.findByText("界面设计师");
    fireEvent.click(await screen.findByRole("button", { name: "Alpha Agent 团队" }));
    await screen.findByText("Alpha Agent 团队");
    await waitFor(() => expect(port.loadTeam).toHaveBeenCalledWith("owner-1", team.id));
    expect(await screen.findByRole("region", { name: "Agent 团队成员" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "邀请 Agent" }));
    expect(await screen.findByRole("dialog", { name: "邀请 Agent" })).toBeVisible();
    expect(screen.getByRole("option", { name: /后端工程师/ })).toHaveAttribute("aria-selected", "true");
    fireEvent.click(screen.getByRole("button", { name: "邀请" }));

    await waitFor(() => expect(port.inviteAgentToTeam).toHaveBeenCalledWith(
      "owner-1", team.id, "agent-2",
    ));
    expect(await screen.findByText(/负责后端服务/)).toBeVisible();
    expect(within(screen.getByRole("region", { name: "Agent 团队成员" })).getByText("2")).toBeVisible();
  });

  it("edits team-scoped duties and assigns a qualified project manager", async () => {
    const team = {
      id: "11111111-1111-4111-8111-111111111111",
      projectId: "project-1",
      name: "Alpha Agent 团队",
      goal: "交付 Alpha",
      defaultAgentId: "agent-1",
      projectManagerAgentId: null,
      memberCount: 1,
      createdAtUnixMs: 10,
      updatedAtUnixMs: 20,
    };
    const manager = { ...profile(), name: "项目经理", professionKey: "project_manager" as const };
    const initialDetail = {
      team,
      members: [{
        agentId: "agent-1", role: "交付负责人", responsibility: "负责项目交付", joinedAtUnixMs: 20,
      }],
    };
    const updatedDetail = {
      team,
      members: [{ ...initialDetail.members[0]!, role: "项目统筹", responsibility: "维护计划与风险" }],
    };
    const managedDetail = {
      ...updatedDetail,
      team: { ...team, projectManagerAgentId: "agent-1", updatedAtUnixMs: 30 },
    };
    const port = stubPort({
      listProfiles: vi.fn(async () => [manager]),
      listTeams: vi.fn(async () => [team]),
      loadTeam: vi.fn(async () => initialDetail),
      updateTeamMember: vi.fn(async () => updatedDetail),
      setTeamProjectManager: vi.fn(async () => managedDetail),
    });
    render(
      <AgentManagementView
        ownerUserId="owner-1"
        port={port}
        projects={[{ id: "project-1", title: "Alpha" }]}
      />,
    );
    await screen.findByRole("heading", { name: "项目经理", level: 3 });
    fireEvent.click(await screen.findByRole("button", { name: "Alpha Agent 团队" }));
    fireEvent.click(await screen.findByRole("button", { name: "管理" }));
    expect(await screen.findByRole("dialog", { name: "管理团队成员" })).toBeVisible();
    fireEvent.change(screen.getByLabelText("当前项目角色"), { target: { value: " 项目统筹 " } });
    fireEvent.change(screen.getByLabelText("当前项目职责"), { target: { value: " 维护计划与风险 " } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(port.updateTeamMember).toHaveBeenCalledWith(
      "owner-1", team.id, "agent-1", { role: "项目统筹", responsibility: "维护计划与风险" },
    ));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "管理团队成员" })).not.toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "管理" }));
    fireEvent.click(await screen.findByRole("button", { name: "设为项目经理" }));
    await waitFor(() => expect(port.setTeamProjectManager).toHaveBeenCalledWith(
      "owner-1", team.id, "agent-1",
    ));
    expect(await screen.findByText("项目经理", { selector: "em" })).toBeVisible();
  });

  it("sends a team message with a native attachment and shows its real Delivery state", async () => {
    const team = {
      id: "11111111-1111-4111-8111-111111111111",
      projectId: "project-1",
      name: "Alpha Agent 团队",
      goal: "交付 Alpha",
      defaultAgentId: "agent-1",
      projectManagerAgentId: null,
      memberCount: 1,
      createdAtUnixMs: 10,
      updatedAtUnixMs: 20,
    };
    const detail = {
      team,
      members: [{
        agentId: "agent-1", role: "界面设计师", responsibility: "负责产品界面", joinedAtUnixMs: 20,
      }],
    };
    const attachment = {
      id: "attachment-1", name: "设计图.png", mimeType: "image/png",
      kind: "image" as const, origin: "file" as const, byteSize: 1_024,
      previewUrl: "chatos-attachment://localhost/a/b/c",
    };
    const message = {
      id: "message-1", senderKind: "human" as const, senderId: "owner-1",
      content: "开始实现", mentionedAgentIds: [], attachments: [{
        id: "stored-attachment-1", name: attachment.name, mimeType: attachment.mimeType,
        size: attachment.byteSize, kind: attachment.kind,
      }], createdAtUnixMs: 30,
    };
    const pending = {
      id: "delivery-1", messageId: message.id, targetAgentId: "agent-1",
      triggerKind: "default_agent" as const, status: "pending" as const,
      responseMessageId: null, lastError: null, createdAtUnixMs: 30, completedAtUnixMs: null,
    };
    const loadTeamConversation = vi.fn()
      .mockResolvedValueOnce({ teamId: team.id, messages: [], deliveries: [] })
      .mockResolvedValueOnce({ teamId: team.id, messages: [message], deliveries: [pending] })
      .mockResolvedValueOnce({
        teamId: team.id, messages: [message], deliveries: [{ ...pending, status: "running" as const }],
      });
    const queueTeamMessage = vi.fn(async () => ({
      messageId: message.id, teamId: team.id, createdAtUnixMs: 30,
      deliveries: [{
        deliveryId: pending.id, targetAgentId: "agent-1", triggerKind: "default_agent" as const,
      }],
    }));
    const dispatchNextTeamDelivery = vi.fn()
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce({
        deliveryId: pending.id, teamId: team.id, agentId: "agent-1", turnId: "turn-1", runId: "run-1",
      })
      .mockResolvedValueOnce(null);
    const port = stubPort({
      listTeams: vi.fn(async () => [team]),
      loadTeam: vi.fn(async () => detail),
      loadTeamConversation,
      queueTeamMessage,
      dispatchNextTeamDelivery,
      loadTeamMemberActivity: vi.fn(async () => ({
        roomId: team.id, agentId: "agent-1", conversationId: `agent-team-${team.id}-agent-1`,
        conversationVersion: 1, hasActiveDelivery: false, items: [],
      })),
    });
    const attachmentPort = {
      activateScope: vi.fn(async () => undefined),
      deactivateScope: vi.fn(async () => undefined),
      pickFiles: vi.fn(async (scope) => ({ scope, attachments: [attachment], errors: [] })),
      importClipboard: vi.fn(),
      discard: vi.fn(async () => undefined),
      previewText: vi.fn(),
      loadPersisted: vi.fn(),
      previewPersistedText: vi.fn(),
      onStaged: vi.fn(async () => () => undefined),
      onDragState: vi.fn(async () => () => undefined),
    } satisfies ConversationAttachmentPort;
    render(
      <AgentManagementView
        attachmentPort={attachmentPort}
        ownerUserId="owner-1"
        port={port}
        projects={[{ id: "project-1", title: "Alpha" }]}
      />,
    );
    await screen.findByText("界面设计师");
    fireEvent.click(await screen.findByRole("button", { name: "Alpha Agent 团队" }));
    fireEvent.click(await screen.findByRole("button", { name: "聊天" }));
    expect(await screen.findByRole("region", { name: "Agent 团队聊天" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "添加团队附件" }));
    expect(await screen.findByText("设计图.png")).toBeVisible();
    fireEvent.change(screen.getByLabelText("团队消息"), { target: { value: " 开始实现 " } });
    fireEvent.click(screen.getByRole("button", { name: "发送团队消息" }));

    await waitFor(() => expect(queueTeamMessage).toHaveBeenCalledWith(
      "owner-1", team.id, "开始实现", [], [attachment.id],
    ));
    expect(await screen.findByText("开始实现")).toBeVisible();
    expect(await screen.findByText("界面设计师 · 正在执行")).toBeVisible();
    expect(screen.getByText("1.0 KB")).toBeVisible();
    expect(screen.getByLabelText("团队消息")).toHaveValue("");
  });

  it("renders the persisted project dashboard with milestones, brief, and Human issues", async () => {
    const team = {
      id: "11111111-1111-4111-8111-111111111111", projectId: "project-1",
      name: "Alpha Agent 团队", goal: "交付 Alpha", defaultAgentId: "agent-1",
      projectManagerAgentId: "agent-1", memberCount: 1, createdAtUnixMs: 10, updatedAtUnixMs: 20,
    };
    const detail = {
      team,
      members: [{ agentId: "agent-1", role: "项目经理", responsibility: "负责交付", joinedAtUnixMs: 20 }],
    };
    const loadTeamDashboard = vi.fn(async () => ({
      dashboard: {
        teamId: team.id, phase: "实现与联调", health: "at_risk" as const,
        summary: "核心链路已完成，等待产品方向确认。", nextSteps: ["确认发布范围", "完成回归"],
        milestones: [{
          id: "milestone-1", title: "Windows 首版", detail: "完成原生打包",
          status: "in_progress" as const, progressPercent: 72, acceptanceCriteria: ["安装包可运行"],
          linkedTodoIds: [], targetAtUnixMs: null,
        }],
        issues: [{
          id: "issue-1", title: "确认发布范围", detail: "需要产品决策",
          requestedAction: "请确认是否包含自动更新", severity: "warning" as const,
          owner: "human" as const, relatedTodoId: null,
        }],
        revision: 3, updatedByAgentId: "agent-1", updatedAtUnixMs: 40,
      },
      pendingApprovalCount: 1,
    }));
    const port = stubPort({
      listProfiles: vi.fn(async () => [{ ...profile(), name: "项目经理", professionKey: "project_manager" as const }]),
      listTeams: vi.fn(async () => [team]), loadTeam: vi.fn(async () => detail), loadTeamDashboard,
      listTeamTodos: vi.fn(async () => []),
      loadTeamRuns: vi.fn(async () => ({
        teamId: team.id, messageCount: 0, pendingDeliveryCount: 0, runningDeliveryCount: 0,
        completedDeliveryCount: 0, failedDeliveryCount: 0, cancelledDeliveryCount: 0, runs: [],
      })),
    });
    renderView(port);
    await screen.findByRole("heading", { name: "项目经理", level: 3 });
    fireEvent.click(await screen.findByRole("button", { name: "Alpha Agent 团队" }));
    const overview = await screen.findByRole("region", { name: "Agent 团队总览" });
    expect(await within(overview).findByText("实现与联调")).toBeVisible();
    expect(within(overview).getByText("有风险")).toBeVisible();
    expect(within(overview).getAllByText("Windows 首版")[0]).toBeVisible();
    expect(within(overview).getByText("核心链路已完成，等待产品方向确认。")).toBeVisible();
    expect(within(overview).getByText("1. 确认发布范围")).toBeVisible();
    expect(within(overview).getByText("请确认是否包含自动更新")).toBeVisible();
    expect(loadTeamDashboard).toHaveBeenCalledWith("owner-1", team.id);
  });

  it("shows native team Run history and filters it by Agent", async () => {
    const team = {
      id: "11111111-1111-4111-8111-111111111111", projectId: "project-1",
      name: "Alpha Agent 团队", goal: "交付 Alpha", defaultAgentId: "agent-1",
      projectManagerAgentId: null, memberCount: 1, createdAtUnixMs: 10, updatedAtUnixMs: 20,
    };
    const detail = {
      team,
      members: [{ agentId: "agent-1", role: "界面设计师", responsibility: "负责产品界面", joinedAtUnixMs: 20 }],
    };
    const loadTeamRuns = vi.fn(async () => ({
      teamId: team.id,
      messageCount: 1,
      pendingDeliveryCount: 0,
      runningDeliveryCount: 0,
      completedDeliveryCount: 1,
      failedDeliveryCount: 0,
      cancelledDeliveryCount: 0,
      runs: [{
        deliveryId: "delivery-1", messageId: "message-1", agentId: "agent-1",
        lane: "manager" as const, triggerKind: "default_agent" as const,
        deliveryStatus: "completed" as const, runStatus: "succeeded" as const,
        turnId: "turn-1", runId: "run-team-1", responseMessageId: "answer-1", lastError: null,
        createdAtUnixMs: 30, updatedAtUnixMs: 40,
      }],
    }));
    const port = stubPort({
      listTeams: vi.fn(async () => [team]), loadTeam: vi.fn(async () => detail), loadTeamRuns,
    });
    const loadRun = vi.fn(async () => ({
      run: {
        runId: "run-team-1", taskId: "task-1", status: "succeeded", iteration: 2,
        maxIterations: 32, resultSummary: "已完成团队回复", reportContent: null,
        errorMessage: null, createdAtUnixMs: 30, updatedAtUnixMs: 40, finishedAtUnixMs: 40,
      },
      events: [{
        cursor: 1, eventId: "event-1", eventType: "run_completed",
        summary: "Run 已完成", status: "succeeded", createdAtUnixMs: 40,
      }],
      eventsHasMore: false,
      nextOffset: 1,
    }));
    const runInspectorPort: RunInspectorPort = {
      loadGraph: vi.fn(), loadTask: vi.fn(), loadRun,
      retryTask: vi.fn(), cancelTask: vi.fn(), restartTask: vi.fn(),
    };
    render(
      <AgentManagementView
        ownerUserId="owner-1"
        port={port}
        projects={[{ id: "project-1", title: "Alpha" }]}
        runInspectorPort={runInspectorPort}
      />,
    );
    await screen.findByText("界面设计师");
    fireEvent.click(await screen.findByRole("button", { name: "Alpha Agent 团队" }));
    fireEvent.click(await screen.findByRole("button", { name: "总览" }));
    const overview = await screen.findByRole("region", { name: "Agent 团队总览" });
    expect(overview).toBeVisible();
    expect(within(overview).getByText("当前阶段")).toBeVisible();
    expect(within(overview).getAllByText("交付 Alpha")[0]).toBeVisible();
    expect(within(overview).getByText("项目经理简报")).toBeVisible();
    fireEvent.click(await screen.findByRole("button", { name: "运行" }));

    await waitFor(() => expect(loadTeamRuns).toHaveBeenCalledWith("owner-1", team.id));
    expect(await screen.findByRole("region", { name: "Agent 团队运行" })).toBeVisible();
    expect(screen.getByText("团队全部运行")).toBeVisible();
    expect(await screen.findByText("默认 Agent · 1 条事件 · 2 次模型调用")).toBeVisible();
    expect(screen.getByText("已完成")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "查看界面设计师的运行状态" }));
    expect(screen.getByText("界面设计师 的运行")).toBeVisible();
    expect(screen.getByText("通讯与任务执行彼此独立，可同时运行")).toBeVisible();
    expect(screen.getByRole("button", { name: "查看全部" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "查看界面设计师的运行详情" }));
    expect(await screen.findByRole("dialog", { name: "团队 Run 检查器" })).toBeVisible();
    await waitFor(() => expect(loadRun).toHaveBeenCalledWith("owner-1", "run-team-1", 0));
    expect(await screen.findByText("Run 已完成")).toBeVisible();
    expect(screen.getByText("已完成团队回复")).toBeVisible();
  });

  it("shows the newest persisted Todo execution record before older history", async () => {
    const team = {
      id: "11111111-1111-4111-8111-111111111111", projectId: "project-1",
      name: "Alpha Agent 团队", goal: "交付 Alpha", defaultAgentId: "agent-1",
      projectManagerAgentId: "agent-1", memberCount: 1, createdAtUnixMs: 10, updatedAtUnixMs: 20,
    };
    const detail = {
      team,
      members: [{ agentId: "agent-1", role: "界面设计师", responsibility: "负责产品界面", joinedAtUnixMs: 20 }],
    };
    const todo = {
      id: "todo-1", agentId: "agent-1", agentName: "界面设计师", teamId: team.id,
      sourceRoomId: team.id, sourceMessageId: "message-1", title: "实现任务面板", detail: "展示真实执行进度",
      priority: 90, sortOrder: 0, status: "in_progress" as const, blockedReason: "", result: "",
      executionContract: {
        objective: "实现任务面板", scope: "Windows React", expectedOutputs: ["任务面板"],
        acceptanceCriteria: ["展示最新记录"], constraints: [],
      },
      executionPlan: {
        requiresExecution: true, builtinCapabilities: ["project_read" as const], plugins: [],
        selectionRevision: "windows-v1", selectedAtUnixMs: 20,
      },
      dependencyIds: [], createdAtUnixMs: 20, updatedAtUnixMs: 40,
    };
    const listTeamTodoProgress = vi.fn(async () => [
      { sequence: 1, runId: "run-1", kind: "started" as const, stage: "准备", detail: "开始读取代码", createdAtUnixMs: 30 },
      { sequence: 2, runId: "run-1", kind: "progress" as const, stage: "实现", detail: "最新：已经完成数据接入", createdAtUnixMs: 40 },
    ]);
    const port = stubPort({
      listTeams: vi.fn(async () => [team]),
      loadTeam: vi.fn(async () => detail),
      listTeamTodos: vi.fn(async () => [todo]),
      listTeamTodoProgress,
      createTeamTodo: vi.fn(), updateTeamTodo: vi.fn(), reorderTeamTodos: vi.fn(),
      cancelTeamTodo: vi.fn(), dispatchNextTeamDelivery: vi.fn(async () => null),
      loadTeamRuns: vi.fn(async () => ({
        teamId: team.id, messageCount: 0, pendingDeliveryCount: 0, runningDeliveryCount: 1,
        completedDeliveryCount: 0, failedDeliveryCount: 0, cancelledDeliveryCount: 0, runs: [],
      })),
    });
    renderView(port);
    await screen.findByText("界面设计师");
    fireEvent.click(await screen.findByRole("button", { name: "Alpha Agent 团队" }));
    fireEvent.click(await screen.findByRole("button", { name: "任务" }));

    expect(await screen.findByRole("region", { name: "团队交付概览" })).toBeVisible();
    expect(screen.getByText("项目文件交付")).toBeVisible();
    expect(screen.queryByRole("button", { name: "新建任务" })).not.toBeInTheDocument();
    expect(await screen.findByText("最新：已经完成数据接入")).toBeVisible();
    expect(listTeamTodoProgress).toHaveBeenCalledWith("owner-1", team.id, "todo-1", 100);
    expect(screen.queryByText("开始读取代码")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看全部 2 条" }));
    expect(await screen.findByText("开始读取代码")).toBeVisible();
    expect(screen.getByText("任务授权")).toBeVisible();
    expect(screen.getByText("独立执行环境")).toBeVisible();
    expect(screen.getByText("Windows React")).toBeVisible();
  });

  it("loads and edits revisioned team shared assets", async () => {
    const team = {
      id: "11111111-1111-4111-8111-111111111111", projectId: "project-1",
      name: "Alpha Agent 团队", goal: "交付 Alpha", defaultAgentId: "agent-1",
      projectManagerAgentId: null, memberCount: 1, createdAtUnixMs: 10, updatedAtUnixMs: 20,
    };
    const detail = {
      team,
      members: [{ agentId: "agent-1", role: "界面设计师", responsibility: "负责产品界面", joinedAtUnixMs: 20 }],
    };
    const asset = {
      id: "asset-1", teamId: team.id, category: "overview" as const, title: "项目概览",
      markdown: "# Alpha\n\n第一版", revision: 1, status: "active" as const,
      createdByAgentId: null, updatedByAgentId: null, createdAtUnixMs: 30, updatedAtUnixMs: 30,
    };
    const saveTeamAsset = vi.fn(async (_owner, _team, input) => ({
      ...asset, category: input.category, title: input.title,
      markdown: input.markdown, revision: 2, updatedAtUnixMs: 40,
    }));
    const port = stubPort({
      listTeams: vi.fn(async () => [team]),
      loadTeam: vi.fn(async () => detail),
      listTeamAssets: vi.fn(async () => [asset]),
      saveTeamAsset,
      archiveTeamAsset: vi.fn(async () => ({ ...asset, revision: 2, status: "archived" as const })),
      listTeamAssetRevisions: vi.fn(async () => [{
        assetId: asset.id, revision: 1, title: asset.title, markdown: asset.markdown,
        editorAgentId: null, createdAtUnixMs: 30,
      }]),
    });
    render(
      <AgentManagementView
        ownerUserId="owner-1"
        port={port}
        projects={[{ id: "project-1", title: "Alpha" }]}
      />,
    );
    await screen.findByText("界面设计师");
    fireEvent.click(await screen.findByRole("button", { name: "Alpha Agent 团队" }));
    fireEvent.click(await screen.findByRole("button", { name: "共享资产" }));
    const workspace = await screen.findByRole("region", { name: "Agent 团队共享资产" });
    expect(within(workspace).getAllByText("项目概览")[0]).toBeVisible();
    expect(within(workspace).getByText("1–1 / 1")).toBeVisible();
    fireEvent.click(within(workspace).getByRole("button", { name: "项目概览" }));
    expect(within(workspace).getByRole("heading", { name: "Alpha", level: 1 })).toBeVisible();
    fireEvent.click(within(workspace).getByRole("menuitem", { name: "编辑" }));
    const editor = await screen.findByRole("dialog", { name: "编辑共享资产" });
    fireEvent.change(within(editor).getByLabelText("标题"), { target: { value: "Alpha 项目概览" } });
    fireEvent.click(within(editor).getByRole("button", { name: "保存" }));
    await waitFor(() => expect(saveTeamAsset).toHaveBeenCalledWith("owner-1", team.id, {
      assetId: "asset-1", category: "overview", title: "Alpha 项目概览",
      markdown: "# Alpha\n\n第一版", expectedRevision: 1,
    }));
    expect(await within(workspace).findByText("Alpha 项目概览")).toBeVisible();
    fireEvent.click(within(workspace).getByRole("menuitem", { name: "版本历史" }));
    const history = await screen.findByRole("dialog", { name: "共享资产版本历史" });
    expect(within(history).getByText(/r1 · 你 ·/)).toBeVisible();
    expect(within(history).getByRole("heading", { name: "Alpha", level: 1 })).toBeVisible();
  });

  it("routes migrated team surveys to the global requirement-survey workspace", async () => {
    const team = {
      id: "11111111-1111-4111-8111-111111111111", projectId: "project-1",
      name: "Alpha Agent 团队", goal: "交付 Alpha", defaultAgentId: "agent-1",
      projectManagerAgentId: null, memberCount: 1, createdAtUnixMs: 10, updatedAtUnixMs: 20,
    };
    const detail = {
      team,
      members: [{ agentId: "agent-1", role: "界面设计师", responsibility: "负责产品界面", joinedAtUnixMs: 20 }],
    };
    const listProjectRequirementSurveys = vi.fn();
    const onOpenRequirementSurveys = vi.fn();
    const port = stubPort({
      listTeams: vi.fn(async () => [team]), loadTeam: vi.fn(async () => detail),
      listProjectRequirementSurveys,
    });
    render(
      <AgentManagementView
        onOpenRequirementSurveys={onOpenRequirementSurveys}
        ownerUserId="owner-1"
        port={port}
        projects={[{ id: "project-1", title: "Alpha" }]}
      />,
    );
    await screen.findByText("界面设计师");
    fireEvent.click(await screen.findByRole("button", { name: "Alpha Agent 团队" }));
    fireEvent.click(await screen.findByRole("button", { name: "需求调研" }));
    const workspace = await screen.findByRole("region", { name: "Agent 团队需求调研" });
    expect(within(workspace).getByText("需求调研已迁移")).toBeVisible();
    expect(within(workspace).getByText("Task 调研现在由 Local Agent Host 保存并恢复原 Run，不再属于 Agent 群聊。")).toBeVisible();
    fireEvent.click(within(workspace).getByRole("button", { name: "打开需求调研" }));
    expect(onOpenRequirementSurveys).toHaveBeenCalledTimes(1);
    expect(listProjectRequirementSurveys).not.toHaveBeenCalled();
  });

  it("selects an Agent and loads its existing private-chat activity without opening a conversation", async () => {
    const loadProfileActivity = vi.fn(async () => ({
      agentId: "agent-1",
      hasActiveDelivery: false,
      items: [],
    }));
    const port = stubPort({ loadProfileActivity });
    renderView(port);
    await screen.findByText("界面设计师");

    fireEvent.click(screen.getByText("界面设计师").closest("article")!);

    await waitFor(() => expect(loadProfileActivity).toHaveBeenCalledWith("owner-1", "agent-1"));
    expect(screen.getByRole("region", { name: "界面设计师 运行动态" })).toBeVisible();
    expect(screen.getByText("最近的私聊消息处理")).toBeVisible();
    expect(await screen.findByText("还没有私聊运行记录")).toBeVisible();
    expect(port.prepareConversation).toBeUndefined();
  });

  it("shows active profile Delivery and Run state", async () => {
    const port = stubPort({
      loadProfileActivity: vi.fn(async () => ({
        agentId: "agent-1",
        hasActiveDelivery: true,
        items: [activityItem("agent-1", "run-active", "running")],
      })),
    });
    renderView(port);
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByText("界面设计师").closest("article")!);

    expect(await screen.findByText("正在运行")).toBeVisible();
    expect(screen.getByText("Run 正在执行")).toBeVisible();
    expect(screen.getByText(/Run run-active/)).toBeVisible();
  });

  it("does not let a late profile response overwrite the newly selected Agent", async () => {
    const first = deferred<AgentProfileActivitySnapshot>();
    const second = deferred<AgentProfileActivitySnapshot>();
    const secondProfile = { ...profile(), id: "agent-2", name: "后端工程师" };
    const loadProfileActivity = vi.fn((_owner: string, agentId: string) => (
      agentId === "agent-1" ? first.promise : second.promise
    ));
    const port = stubPort({
      listProfiles: vi.fn(async () => [profile(), secondProfile]),
      loadProfileActivity,
    });
    renderView(port);
    await screen.findByText("后端工程师");

    const firstCard = screen.getByText("界面设计师").closest("article")!;
    const secondCard = screen.getByText("后端工程师").closest("article")!;
    fireEvent.click(firstCard);
    await waitFor(() => expect(loadProfileActivity).toHaveBeenCalledWith("owner-1", "agent-1"));
    fireEvent.click(secondCard);
    await waitFor(() => expect(loadProfileActivity).toHaveBeenCalledWith("owner-1", "agent-2"));
    second.resolve({
      agentId: "agent-2",
      hasActiveDelivery: false,
      items: [activityItem("agent-2", "run-second", "completed")],
    });
    expect(await screen.findByText(/Run run-second/)).toBeVisible();

    first.resolve({
      agentId: "agent-1",
      hasActiveDelivery: false,
      items: [activityItem("agent-1", "run-first", "completed")],
    });
    await first.promise;
    await waitFor(() => {
      expect(screen.getByRole("region", { name: "后端工程师 运行动态" })).toBeVisible();
      expect(screen.queryByText(/Run run-first/)).not.toBeInTheDocument();
    });
  });

  it("opens the native prepared conversation when all Agent chat dependencies are available", async () => {
    const roomId = "11111111-1111-4111-8111-111111111111";
    const prepareConversation = vi.fn(async () => ({
      roomId,
      agentId: "agent-1",
      conversationId: `agent-direct-${roomId}`,
      title: "界面设计师",
      createdAtUnixMs: 10,
      updatedAtUnixMs: 20,
    }));
    const port = stubPort({
      prepareConversation,
      sendConversationDraft: vi.fn(),
      loadConversationActivity: vi.fn(async () => ({
        roomId,
        agentId: "agent-1",
        conversationId: `agent-direct-${roomId}`,
        conversationVersion: 2,
        hasActiveDelivery: false,
        items: [{
          deliveryId: "delivery-1",
          roomId,
          agentId: "agent-1",
          turnId: "turn-1",
          runId: "run-1",
          deliveryStatus: "completed" as const,
          runStatus: "succeeded" as const,
          responseMessageId: "answer-1",
          createdAtUnixMs: 10,
          updatedAtUnixMs: 20,
        }],
      })),
    });
    render(
      <AgentManagementView
        askUserPort={{} as never}
        attachmentPort={{} as never}
        historyPort={{} as never}
        ownerUserId="owner-1"
        port={port}
        realtimePort={{} as never}
        runInspectorPort={{} as never}
        runtimeSettingsPort={{} as never}
      />,
    );
    await screen.findByText("界面设计师");

    const directButton = screen.getByRole("button", { name: "私聊" });
    expect(directButton).toBeEnabled();
    fireEvent.click(directButton);

    await waitFor(() => expect(prepareConversation).toHaveBeenCalledWith("owner-1", "agent-1"));
    expect(await screen.findByLabelText("界面设计师 Agent 私聊")).toBeVisible();
    expect(screen.getByTestId("agent-conversation-history")).toHaveAttribute("data-agent-bound", "true");
    expect(screen.getByRole("button", { name: "返回 Agent 管理" })).toBeEnabled();
    expect(await screen.findByText("最近 1 次运行")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "查看动态" }));
    expect(screen.getByText("Run 已完成")).toBeVisible();
    expect(screen.getByText(/Delivery delivery-1/)).toBeVisible();
  });

  it("shows complete staffing facts and sends the Human approval decision", async () => {
    const roomId = "11111111-1111-4111-8111-111111111111";
    const proposal = {
      id: "proposal-1",
      kind: "agentCreation" as const,
      sourceRoomId: roomId,
      proposerAgentId: "agent-1",
      status: "pending" as const,
      name: "后端工程师",
      role: "服务端负责人",
      responsibility: "负责服务端交付",
      rolePrompt: "严格参考现有架构完成服务端任务",
      thinkingLevel: "high" as const,
      professionKey: "backend_engineer" as const,
      rationale: "团队存在长期后端职责缺口",
      targetTeamId: null,
      targetAgentId: null,
      reason: null,
      handoffPlan: null,
      createdAtUnixMs: 30,
    };
    const resolveStaffingProposal = vi.fn(async () => ({
      proposalId: proposal.id,
      kind: proposal.kind,
      status: "approved" as const,
      sourceRoomId: roomId,
      affectedTeamId: null,
      createdAgentId: "agent-created",
    }));
    const port = stubPort({
      prepareConversation: vi.fn(async () => ({
        roomId, agentId: "agent-1", conversationId: `agent-direct-${roomId}`,
        title: "界面设计师", createdAtUnixMs: 10, updatedAtUnixMs: 20,
      })),
      loadConversationActivity: vi.fn(async () => ({
        roomId, agentId: "agent-1", conversationId: `agent-direct-${roomId}`,
        conversationVersion: 1, hasActiveDelivery: false, items: [],
      })),
      listStaffingProposals: vi.fn(async () => [proposal]),
      resolveStaffingProposal,
      sendConversationDraft: vi.fn(),
    });
    render(
      <AgentManagementView
        askUserPort={{} as never}
        attachmentPort={{} as never}
        historyPort={{} as never}
        ownerUserId="owner-1"
        port={port}
        realtimePort={{} as never}
        runInspectorPort={{} as never}
        runtimeSettingsPort={{} as never}
      />,
    );
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "私聊" }));
    expect(await screen.findByRole("region", { name: "待审批人员提案" })).toBeVisible();
    expect(await screen.findByText("创建 Agent：后端工程师")).toBeVisible();
    expect(screen.getByText("负责服务端交付")).toBeVisible();
    expect(screen.getByText("团队存在长期后端职责缺口")).toBeVisible();
    fireEvent.click(screen.getByText("查看完整角色提示词"));
    expect(screen.getByText("严格参考现有架构完成服务端任务")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "批准" }));
    await waitFor(() => expect(resolveStaffingProposal).toHaveBeenCalledWith(
      "owner-1", roomId, proposal.id, proposal.kind, true,
    ));
    await waitFor(() => expect(screen.queryByRole("region", { name: "待审批人员提案" })).not.toBeInTheDocument());
  });

  it("edits the macOS-compatible bilingual Skill catalog", async () => {
    const item = {
      kind: "profession" as const,
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
      ...skillProjection("profession", "project_manager"),
    };
    const port = stubPort({
      loadSkillCatalog: vi.fn(async () => [item]),
      saveSkillOverride: vi.fn(async (_owner, input) => ({ ...item, ...input, customized: true })),
      resetSkillOverride: vi.fn(async () => item),
    });
    const onContextLanguageChange = vi.fn();
    render(
      <AgentManagementView
        contextLanguage="zh-CN"
        onContextLanguageChange={onContextLanguageChange}
        ownerUserId="owner-1"
        port={port}
      />,
    );
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "Skill 管理" }));
    expect(await screen.findByRole("dialog", { name: "Skill 管理" })).toBeVisible();
    expect(screen.getByLabelText("名称")).toHaveValue("项目经理");
    fireEvent.click(screen.getByRole("button", { name: "参考资料" }));
    expect(screen.getByRole("listbox", { name: "Skill 参考资料" })).toBeVisible();
    expect(screen.getByText("4 bytes")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "English" }));
    expect(onContextLanguageChange).toHaveBeenCalledWith("en-US");
    fireEvent.change(screen.getByLabelText("名称"), { target: { value: "Delivery Lead" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(port.saveSkillOverride).toHaveBeenCalledWith("owner-1", expect.objectContaining({
      key: "project_manager",
      label: "项目经理",
      labelEn: "Delivery Lead",
      markdown: "# 项目经理",
      markdownEn: "# Project Manager",
    })));
  });
});

function renderView(port = stubPort()) {
  return render(<AgentManagementView ownerUserId="owner-1" port={port} />);
}

function activityItem(
  agentId: string,
  runId: string,
  deliveryStatus: "running" | "completed",
): AgentProfileActivitySnapshot["items"][number] {
  return {
    deliveryId: `delivery-${runId}`,
    roomId: agentId === "agent-1"
      ? "11111111-1111-4111-8111-111111111111"
      : "22222222-2222-4222-8222-222222222222",
    agentId,
    turnId: `turn-${runId}`,
    runId,
    deliveryStatus,
    runStatus: deliveryStatus === "running" ? "running" : "succeeded",
    responseMessageId: deliveryStatus === "running" ? null : `message-${runId}`,
    createdAtUnixMs: 10,
    updatedAtUnixMs: 20,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}

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

function stubPort(overrides: Partial<AgentManagementPort> = {}): AgentManagementPort {
  return {
    listProfiles: vi.fn(async () => [profile()]),
    loadEditorCatalog: vi.fn(async () => catalog()),
    loadProfileEditor: vi.fn(async () => ({
      profile: profile(), rolePrompt: "私有角色 Prompt", heartbeatPrompt: "检查阻塞和待推进工作",
    })),
    saveProfile: vi.fn(async (_ownerUserId, input) => ({
      ...profile(), id: input.agentId ?? "agent-created", name: input.name,
      description: input.description, modelConfigId: input.modelConfigId,
      thinkingLevel: input.thinkingLevel, professionKey: input.professionKey,
      heartbeatEnabled: input.heartbeatEnabled, heartbeatIntervalSeconds: input.heartbeatIntervalSeconds,
      canManageStaff: input.canManageStaff, canAccessLocalProjects: input.canAccessLocalProjects,
      updatedAtUnixMs: 30,
    })),
    pickAvatar: vi.fn(async () => null),
    loadAvatarHistory: vi.fn(async () => []),
    discardAvatarReferences: vi.fn(async () => undefined),
    discardAvatarPresentation: vi.fn(async () => undefined),
    ...overrides,
  };
}

function catalog(): AgentProfileEditorCatalog {
  return {
    models: [{
      id: "model-1", displayName: "主要模型", modelName: "gpt-test", provider: "openai",
      defaultThinkingLevel: "high", supportsReasoning: true, thinkingLevels: ["auto", "high"],
    }],
    professions: Object.entries(agentProfessionLabels).map(([key, label]) => ({
      key, label, description: `${label}职责`, categoryKey: "general", categoryLabel: "通用",
    })) as AgentProfileEditorCatalog["professions"],
    defaultRolePrompt: "默认角色 Prompt",
  };
}

function profile(): AgentProfileSummary {
  return {
    id: "agent-1", name: "界面设计师", description: "负责产品界面与视觉规范",
    modelConfigId: "model-1", thinkingLevel: "high", professionKey: "ui_designer",
    heartbeatEnabled: true, heartbeatIntervalSeconds: 900, lastHeartbeatAtUnixMs: 100,
    nextHeartbeatAtUnixMs: 200, canManageStaff: false, canAccessLocalProjects: true,
    hasAvatar: false, createdAtUnixMs: 10, updatedAtUnixMs: 20,
    avatarPreviewUrl: null,
  };
}
