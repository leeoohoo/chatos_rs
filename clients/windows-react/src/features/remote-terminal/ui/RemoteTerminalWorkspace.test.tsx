import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { forwardRef, useImperativeHandle } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RemoteTerminalPoll } from "../domain/remoteTerminal";
import type { RemoteTerminalPort } from "../ports/RemoteTerminalPort";
import { useRemoteTerminalStore } from "../application/remoteTerminalStore";
import { RemoteTerminalWorkspace } from "./RemoteTerminalWorkspace";

vi.mock("./RemoteTerminalSurface", () => ({
  RemoteTerminalSurface: forwardRef(function MockSurface(_props, ref) {
    useImperativeHandle(ref, () => ({ clear: vi.fn(), findNext: vi.fn(), findPrevious: vi.fn(), focus: vi.fn() }));
    return <div role="application" aria-label="远程终端输出" />;
  }),
}));

beforeEach(() => useRemoteTerminalStore.setState({ ownerUserId: null, presentationId: null, sessionId: null, phase: "idle", activeAction: null, verificationPrompt: null, errorMessage: null }));
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("RemoteTerminalWorkspace", () => {
  it("shows MFA and reconnects the real native session with the entered code", async () => {
    const pendingPoll = new Promise<RemoteTerminalPoll>(() => undefined);
    const start = vi.fn()
      .mockResolvedValueOnce({ status: "verification_required", prompt: "One-time password:" })
      .mockResolvedValueOnce({ status: "started", sessionId: "remote-terminal-1", initialWorkingDirectory: "/srv/app" });
    const adapter = port({ start, poll: vi.fn(() => pendingPoll) });
    renderWorkspace(adapter);
    expect(await screen.findByText("One-time password:")).toBeVisible();
    fireEvent.change(screen.getByLabelText("验证码"), { target: { value: "654321" } });
    fireEvent.click(screen.getByRole("button", { name: "验证并连接" }));
    await waitFor(() => expect(start).toHaveBeenLastCalledWith(expect.objectContaining({ verificationCode: "654321" })));
    expect(await screen.findByText("已连接")).toBeVisible();
    expect(screen.getByText("/srv/app")).toBeVisible();
  });

  it("exposes interrupt and explicit disconnect only while connected", async () => {
    const pendingPoll = new Promise<RemoteTerminalPoll>(() => undefined);
    const adapter = port({ poll: vi.fn(() => pendingPoll) });
    renderWorkspace(adapter);
    await screen.findByText("已连接");
    fireEvent.click(screen.getByRole("button", { name: "中断" }));
    await waitFor(() => expect(adapter.interrupt).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "remote-terminal-1" })));
    fireEvent.click(screen.getByRole("button", { name: "断开" }));
    await waitFor(() => expect(adapter.close).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "remote-terminal-1" })));
    expect(await screen.findByText("已断开")).toBeVisible();
  });
});

function renderWorkspace(adapter: RemoteTerminalPort) {
  return render(<RemoteTerminalWorkspace connectionId="remote-1" connectionName="Production" initialWorkingDirectory="~" ownerUserId="alice" port={adapter} />);
}

function port(overrides: Partial<RemoteTerminalPort> = {}): RemoteTerminalPort {
  const pending = new Promise<RemoteTerminalPoll>(() => undefined);
  return {
    start: vi.fn(async () => ({ status: "started" as const, sessionId: "remote-terminal-1", initialWorkingDirectory: "~" })),
    sendInput: vi.fn(async () => undefined),
    resize: vi.fn(async () => undefined),
    interrupt: vi.fn(async () => undefined),
    poll: vi.fn(() => pending),
    close: vi.fn(async () => undefined),
    discardPresentation: vi.fn(async () => undefined),
    ...overrides,
  };
}
