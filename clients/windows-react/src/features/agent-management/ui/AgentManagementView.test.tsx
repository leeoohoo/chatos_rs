import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentManagementStore } from "../application/agentManagementStore";
import {
  agentProfessionLabels,
  type AgentProfileEditorCatalog,
  type AgentProfileSummary,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { AgentManagementView } from "./AgentManagementView";

describe("AgentManagementView", () => {
  beforeEach(() => useAgentManagementStore.getState().reset());
  afterEach(cleanup);

  it("renders macOS profile card facts and enables profile editing", async () => {
    renderView();
    expect(await screen.findByText("界面设计师")).toBeVisible();
    expect(screen.getByText("UI 设计师")).toBeVisible();
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
    fireEvent.click(screen.getByRole("button", { name: /创建 Agent/ }));
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
    fireEvent.click(screen.getByRole("button", { name: /创建 Agent/ }));
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
});

function renderView(port = stubPort()) {
  return render(<AgentManagementView ownerUserId="owner-1" port={port} />);
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
