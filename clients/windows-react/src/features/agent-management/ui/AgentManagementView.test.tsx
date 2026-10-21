import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentManagementStore } from "../application/agentManagementStore";
import { useAgentSkillManagementStore } from "../application/agentSkillManagementStore";
import { useAgentTeamConversationStore } from "../application/agentTeamConversationStore";
import { useAgentTeamRunStore } from "../application/agentTeamRunStore";
import { useAgentTeamStore } from "../application/agentTeamStore";
import {
  agentProfessionLabels,
  type AgentProfileEditorCatalog,
  type AgentProfileActivitySnapshot,
  type AgentProfileSummary,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import type { RunInspectorPort } from "../../run-inspector/ports/RunInspectorPort";
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
  afterEach(cleanup);

  it("renders macOS profile card facts and enables profile editing", async () => {
    renderView();
    expect(await screen.findByText("界面设计师")).toBeVisible();
    expect(screen.getByText("UI 设计师")).toBeVisible();
    expect(await screen.findByText("主要模型")).toBeVisible();
    expect(screen.getByText("每 15 分钟")).toBeVisible();
    expect(screen.getByText("项目与团队")).toBeVisible();
    expect(screen.getByRole("button", { name: /创建 Agent/ })).toBeEnabled();
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

  it("opens creation with macOS defaults", async () => {
    renderView();
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getAllByRole("button", { name: /创建 Agent/ })[0]!);
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
    fireEvent.click(screen.getAllByRole("button", { name: /创建 Agent/ })[1]!);
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

  it("refreshes the native profile truth", async () => {
    const port = stubPort();
    renderView(port);
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "刷新 Agent" }));
    await waitFor(() => expect(port.listProfiles).toHaveBeenCalledTimes(2));
  });

  it("shows a truthful empty state", async () => {
    renderView(stubPort({ listProfiles: vi.fn(async () => []) }));
    expect(await screen.findByText("还没有 Agent")).toBeVisible();
    expect(screen.getByText(/创建一个 Agent/)).toBeVisible();
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
    fireEvent.click(screen.getByRole("button", { name: "团队" }));
    expect(await screen.findByText("还没有项目团队")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "＋ 创建团队" }));
    expect(await screen.findByRole("dialog", { name: "创建 Agent 团队" })).toBeVisible();
    expect(screen.getByLabelText("绑定项目")).toHaveValue("project-1");
    fireEvent.click(screen.getByRole("button", { name: "创建团队" }));

    await waitFor(() => expect(port.createTeam).toHaveBeenCalledWith("owner-1", {
      projectId: "project-1",
      name: "Alpha Agent 团队",
      goal: "",
    }));
    expect(await screen.findByText("Alpha Agent 团队")).toBeVisible();
    expect(screen.getByText("0 位 Agent")).toBeVisible();
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
    fireEvent.click(screen.getByRole("button", { name: "团队" }));
    await screen.findByText("Alpha Agent 团队");
    fireEvent.click(await screen.findByRole("button", { name: "查看成员" }));
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
    expect(screen.getByText("2 位 Agent")).toBeVisible();
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
    fireEvent.click(screen.getByRole("button", { name: "团队" }));
    fireEvent.click(await screen.findByRole("button", { name: "查看成员" }));
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

  it("sends a team message through the native queue and shows its real Delivery state", async () => {
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
    const message = {
      id: "message-1", senderKind: "human" as const, senderId: "owner-1",
      content: "开始实现", mentionedAgentIds: [], createdAtUnixMs: 30,
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
    render(
      <AgentManagementView
        ownerUserId="owner-1"
        port={port}
        projects={[{ id: "project-1", title: "Alpha" }]}
      />,
    );
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "团队" }));
    fireEvent.click(await screen.findByRole("button", { name: "查看成员" }));
    fireEvent.click(await screen.findByRole("button", { name: "聊天" }));
    expect(await screen.findByRole("region", { name: "Agent 团队聊天" })).toBeVisible();
    fireEvent.change(screen.getByLabelText("团队消息"), { target: { value: " 开始实现 " } });
    fireEvent.click(screen.getByRole("button", { name: "发送团队消息" }));

    await waitFor(() => expect(queueTeamMessage).toHaveBeenCalledWith(
      "owner-1", team.id, "开始实现", [],
    ));
    expect(await screen.findByText("开始实现")).toBeVisible();
    expect(await screen.findByText("界面设计师 · 正在执行")).toBeVisible();
    expect(screen.getByLabelText("团队消息")).toHaveValue("");
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
    fireEvent.click(screen.getByRole("button", { name: "团队" }));
    fireEvent.click(await screen.findByRole("button", { name: "查看成员" }));
    fireEvent.click(await screen.findByRole("button", { name: "总览" }));
    const overview = await screen.findByRole("region", { name: "Agent 团队总览" });
    expect(overview).toBeVisible();
    expect(within(overview).getByText("团队目标")).toBeVisible();
    expect(within(overview).getByText("交付 Alpha")).toBeVisible();
    expect(within(overview).getByRole("button", { name: /1\s*团队消息/ })).toBeVisible();
    fireEvent.click(await screen.findByRole("button", { name: "运行" }));

    await waitFor(() => expect(loadTeamRuns).toHaveBeenCalledWith("owner-1", team.id));
    expect(await screen.findByRole("region", { name: "Agent 团队运行" })).toBeVisible();
    expect(screen.getByText("默认 Agent · Run run-team-1")).toBeVisible();
    expect(screen.getByText("已完成")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "界面设计师" }));
    expect(screen.getByText("Run run-team-1", { exact: false })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "查看详情" }));
    expect(await screen.findByRole("dialog", { name: "团队 Run 检查器" })).toBeVisible();
    await waitFor(() => expect(loadRun).toHaveBeenCalledWith("owner-1", "run-team-1", 0));
    expect(await screen.findByText("Run 已完成")).toBeVisible();
    expect(screen.getByText("已完成团队回复")).toBeVisible();
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
    fireEvent.click(screen.getByRole("button", { name: "运行动态" }));

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

    const activityButtons = screen.getAllByRole("button", { name: "运行动态" });
    fireEvent.click(activityButtons[0]!);
    await waitFor(() => expect(loadProfileActivity).toHaveBeenCalledWith("owner-1", "agent-1"));
    fireEvent.click(activityButtons[1]!);
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
    renderView(port);
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "Skill 管理" }));
    expect(await screen.findByRole("dialog", { name: "Skill 管理" })).toBeVisible();
    expect(screen.getByLabelText("名称")).toHaveValue("项目经理");
    fireEvent.click(screen.getByRole("button", { name: "参考资料" }));
    expect(screen.getByRole("listbox", { name: "Skill 参考资料" })).toBeVisible();
    expect(screen.getByText("4 bytes")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "English" }));
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
