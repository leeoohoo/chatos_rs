import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentManagementStore } from "../application/agentManagementStore";
import { useAgentSkillManagementStore } from "../application/agentSkillManagementStore";
import {
  agentProfessionLabels,
  type AgentProfileEditorCatalog,
  type AgentProfileActivitySnapshot,
  type AgentProfileSummary,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { AgentManagementView } from "./AgentManagementView";

vi.mock("../../conversation-history/ui/ConversationHistoryView", () => ({
  ConversationHistoryView: ({ agentBound }: { agentBound?: boolean }) => (
    <div data-testid="agent-conversation-history" data-agent-bound={String(agentBound)} />
  ),
}));

describe("AgentManagementView", () => {
  beforeEach(() => useAgentManagementStore.getState().reset());
  beforeEach(() => useAgentSkillManagementStore.getState().reset());
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
