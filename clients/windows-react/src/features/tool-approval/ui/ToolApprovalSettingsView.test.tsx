import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { sampleToolApproval } from "../../../test/toolApprovalFixture";
import { useToolApprovalSettingsStore } from "../application/toolApprovalSettingsStore";
import { useToolApprovalStore } from "../application/toolApprovalStore";
import type { ToolApprovalSettings } from "../domain/toolApprovalSettings";
import type { ToolApprovalPort } from "../ports/ToolApprovalPort";
import type { ToolApprovalSettingsPort } from "../ports/ToolApprovalSettingsPort";
import { ToolApprovalSettingsView } from "./ToolApprovalSettingsView";

describe("ToolApprovalSettingsView", () => {
  beforeEach(() => {
    useToolApprovalSettingsStore.getState().reset();
    useToolApprovalStore.getState().reset();
  });
  afterEach(cleanup);

  it("waits for the Host before loading local security settings", () => {
    const settingsPort = stubSettingsPort();
    renderView({ hostReady: false, settingsPort });
    expect(screen.getByText("本地服务尚未就绪")).toBeVisible();
    expect(settingsPort.loadSettings).not.toHaveBeenCalled();
  });

  it("confirms elevated modes and surfaces the authoritative missing-model error", async () => {
    const settingsPort = stubSettingsPort({
      updateMode: vi.fn(async () => {
        throw new Error("请先选择本机审批 Agent 模型。");
      }),
    });
    renderView({ settingsPort });
    await screen.findByText("默认审批级别");

    fireEvent.click(screen.getByRole("button", { name: "自动审批" }));
    expect(settingsPort.updateMode).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "确认更改审批模式" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "启用自动审批" }));

    await waitFor(() => expect(settingsPort.updateMode).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      mode: "auto_approval",
      riskAcknowledged: true,
    }));
    expect(await screen.findByRole("alert")).toHaveTextContent("请先选择本机审批 Agent 模型。");
  });

  it("updates the owner-scoped approval model and reasoning level", async () => {
    const settingsPort = stubSettingsPort({
      updateModel: vi.fn(async (request) => settings({
        selectedModelId: request.selectedModelId,
        selectedThinkingLevel: request.selectedThinkingLevel,
      })),
    });
    renderView({ settingsPort });
    await screen.findByText("本机自动审批 Agent");

    fireEvent.change(screen.getByLabelText("模型"), { target: { value: "model-1" } });
    await waitFor(() => expect(settingsPort.updateModel).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      selectedModelId: "model-1",
      selectedThinkingLevel: "medium",
    }));
    expect(screen.getByLabelText("推理等级")).toHaveValue("medium");

    fireEvent.change(screen.getByLabelText("推理等级"), { target: { value: "high" } });
    await waitFor(() => expect(settingsPort.updateModel).toHaveBeenLastCalledWith({
      ownerUserId: "owner-1",
      selectedModelId: "model-1",
      selectedThinkingLevel: "high",
    }));
  });

  it("resolves a pending command and renders durable approval history", async () => {
    const approval = sampleToolApproval();
    useToolApprovalStore.setState({
      ownerUserId: "owner-1",
      hostAvailable: true,
      approvals: [approval],
      hasLoaded: true,
    });
    const approvalPort: ToolApprovalPort = {
      listPending: vi.fn(async () => []),
      decide: vi.fn(async () => ({
        invocationId: approval.invocationId,
        invocationVersion: approval.invocationVersion + 1,
        decision: "accept" as const,
      })),
    };
    const settingsPort = stubSettingsPort({
      loadSettings: vi.fn(async () => settings({
        history: [{
          id: "history-1",
          invocationId: "previous-invocation",
          invocationVersion: 3,
          command: "npm test",
          purpose: "验证历史记录",
          cwd: "clients/windows-react",
          source: "Task",
          mode: "request_approval",
          decision: "accept",
          risk: "medium",
          reason: null,
          createdAtUnixMs: 1_700_000_000_000,
        }],
      })),
    });
    renderView({ approvalPort, settingsPort });

    expect(await screen.findByText("验证历史记录")).toBeVisible();
    expect(screen.getByText("npm test")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "仅本次允许" }));
    await waitFor(() => expect(approvalPort.decide).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      invocationId: approval.invocationId,
      expectedVersion: approval.invocationVersion,
      decision: "accept",
    }));
    await waitFor(() => expect(screen.queryByText("cargo test")).not.toBeInTheDocument());
  });
});

function renderView({
  hostReady = true,
  approvalPort = stubApprovalPort(),
  settingsPort = stubSettingsPort(),
}: {
  hostReady?: boolean;
  approvalPort?: ToolApprovalPort;
  settingsPort?: ToolApprovalSettingsPort;
} = {}) {
  return render(
    <ToolApprovalSettingsView
      approvalPort={approvalPort}
      hostReady={hostReady}
      ownerUserId="owner-1"
      settingsPort={settingsPort}
    />,
  );
}

function settings(overrides: Partial<ToolApprovalSettings> = {}): ToolApprovalSettings {
  return {
    defaultMode: "request_approval",
    selectedModelId: null,
    selectedThinkingLevel: null,
    models: [{
      id: "model-1",
      displayName: "GPT",
      modelName: "gpt-test",
      provider: "openai",
      supportsReasoning: true,
      thinkingLevels: ["low", "medium", "high"],
      defaultThinkingLevel: "medium",
    }],
    automaticReviewerReady: false,
    automaticReviewerDetail: "自动审批暂时转交用户确认。",
    history: [],
    ...overrides,
  };
}

function stubSettingsPort(
  overrides: Partial<ToolApprovalSettingsPort> = {},
): ToolApprovalSettingsPort {
  return {
    loadSettings: vi.fn(async () => settings()),
    updateMode: vi.fn(async () => settings()),
    updateModel: vi.fn(async () => settings()),
    ...overrides,
  };
}

function stubApprovalPort(): ToolApprovalPort {
  return {
    listPending: vi.fn(async () => []),
    decide: vi.fn(async () => {
      throw new Error("not used");
    }),
  };
}
