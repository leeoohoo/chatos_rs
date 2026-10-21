import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentManagementStore } from "../application/agentManagementStore";
import type { AgentProfileSummary } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { AgentManagementView } from "./AgentManagementView";

describe("AgentManagementView", () => {
  beforeEach(() => useAgentManagementStore.getState().reset());
  afterEach(cleanup);

  it("renders macOS profile card facts without exposing unsupported actions", async () => {
    renderView();
    expect(await screen.findByText("界面设计师")).toBeVisible();
    expect(screen.getByText("UI 设计师")).toBeVisible();
    expect(screen.getByText("每 15 分钟")).toBeVisible();
    expect(screen.getByText("项目与团队")).toBeVisible();
    expect(screen.getByRole("button", { name: /创建 Agent/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "私聊" })).toBeDisabled();
  });

  it("refreshes the native profile truth", async () => {
    const port = stubPort();
    renderView(port);
    await screen.findByText("界面设计师");
    fireEvent.click(screen.getByRole("button", { name: "刷新 Agent" }));
    await waitFor(() => expect(port.listProfiles).toHaveBeenCalledTimes(2));
  });

  it("shows a truthful empty state", async () => {
    renderView(stubPort(vi.fn(async () => [])));
    expect(await screen.findByText("还没有 Agent")).toBeVisible();
    expect(screen.getByText(/创建能力将在下一阶段开放/)).toBeVisible();
  });
});

function renderView(port = stubPort()) {
  return render(<AgentManagementView ownerUserId="owner-1" port={port} />);
}

function stubPort(
  listProfiles: AgentManagementPort["listProfiles"] = vi.fn(async () => [profile()]),
): AgentManagementPort {
  return { listProfiles };
}

function profile(): AgentProfileSummary {
  return {
    id: "agent-1",
    name: "界面设计师",
    description: "负责产品界面与视觉规范",
    modelConfigId: "gpt-main",
    thinkingLevel: "high",
    professionKey: "ui_designer",
    heartbeatEnabled: true,
    heartbeatIntervalSeconds: 900,
    lastHeartbeatAtUnixMs: 100,
    nextHeartbeatAtUnixMs: 200,
    canManageStaff: false,
    canAccessLocalProjects: true,
    hasAvatar: false,
    createdAtUnixMs: 10,
    updatedAtUnixMs: 20,
  };
}
