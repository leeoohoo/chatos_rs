import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { sampleToolApproval } from "../../../test/toolApprovalFixture";
import { useToolApprovalStore } from "../application/toolApprovalStore";
import type { ToolApprovalPort } from "../ports/ToolApprovalPort";
import { GlobalToolApprovalOverlay } from "./GlobalToolApprovalOverlay";

describe("GlobalToolApprovalOverlay", () => {
  beforeEach(() => useToolApprovalStore.getState().reset());
  afterEach(cleanup);

  it("shows the safe command and writes an allow-once decision", async () => {
    const approval = sampleToolApproval();
    const port: ToolApprovalPort = {
      listPending: vi.fn()
        .mockResolvedValueOnce([approval])
        .mockResolvedValueOnce([]),
      decide: vi.fn(async () => ({
        invocationId: approval.invocationId,
        invocationVersion: approval.invocationVersion + 1,
        decision: "accept" as const,
      })),
    };
    render(<GlobalToolApprovalOverlay hostReady ownerUserId="user-1" port={port} />);
    expect(await screen.findByText("cargo test")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "仅本次允许" }));
    await waitFor(() => expect(port.decide).toHaveBeenCalledWith({
      ownerUserId: "user-1",
      invocationId: approval.invocationId,
      expectedVersion: approval.invocationVersion,
      decision: "accept",
    }));
    await waitFor(() => expect(screen.queryByLabelText("命令审批")).not.toBeInTheDocument());
  });

  it("writes the macOS-compatible session decision when the Host publishes it", async () => {
    const approval = sampleToolApproval();
    const port: ToolApprovalPort = {
      listPending: vi.fn()
        .mockResolvedValueOnce([approval])
        .mockResolvedValueOnce([]),
      decide: vi.fn(async () => ({
        invocationId: approval.invocationId,
        invocationVersion: approval.invocationVersion + 1,
        decision: "acceptForSession" as const,
      })),
    };
    render(<GlobalToolApprovalOverlay hostReady ownerUserId="user-1" port={port} />);
    await screen.findByText("cargo test");
    fireEvent.click(screen.getByRole("button", { name: "本次会话允许" }));
    await waitFor(() => expect(port.decide).toHaveBeenCalledWith({
      ownerUserId: "user-1",
      invocationId: approval.invocationId,
      expectedVersion: approval.invocationVersion,
      decision: "acceptForSession",
    }));
  });

  it("automatically expands when a new approval arrives", async () => {
    const first = sampleToolApproval({ invocationId: "a" });
    const second = sampleToolApproval({ invocationId: "b", command: "npm install" });
    let pending = [first];
    const port: ToolApprovalPort = {
      listPending: vi.fn(async () => pending),
      decide: vi.fn(async () => { throw new Error("not used"); }),
    };
    render(<GlobalToolApprovalOverlay hostReady ownerUserId="user-1" port={port} />);
    await screen.findByText("cargo test");
    fireEvent.click(screen.getByRole("button", { name: "收起审批浮层" }));
    expect(screen.getByRole("button", { name: "1 项等待审批，点击展开" })).toBeVisible();
    pending = [first, second];
    await useToolApprovalStore.getState().refresh(port);
    expect(await screen.findByLabelText("命令审批")).toBeVisible();
  });
});
