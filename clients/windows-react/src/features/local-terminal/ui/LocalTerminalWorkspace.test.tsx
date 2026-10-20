import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { forwardRef, useImperativeHandle } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LocalTerminalPoll } from "../domain/localTerminal";
import type { LocalTerminalPort } from "../ports/LocalTerminalPort";
import { useLocalTerminalStore } from "../application/localTerminalStore";
import { LocalTerminalWorkspace } from "./LocalTerminalWorkspace";

vi.mock("./LocalTerminalSurface", () => ({
  LocalTerminalSurface: forwardRef(function MockSurface(_props, ref) {
    useImperativeHandle(ref, () => ({
      clear: vi.fn(),
      findNext: vi.fn(),
      findPrevious: vi.fn(),
      focus: vi.fn(),
    }));
    return <div role="application" aria-label="本机终端输出" />;
  }),
}));

beforeEach(() => useLocalTerminalStore.setState({
  ownerUserId: null,
  hostAvailable: false,
  generation: useLocalTerminalStore.getState().generation + 1,
  tabs: [],
  selectedTabId: null,
}));
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("LocalTerminalWorkspace", () => {
  it("mirrors macOS multi-tab controls and interrupt behavior", async () => {
    const adapter = port();
    render(<LocalTerminalWorkspace hostReady ownerUserId="alice" port={adapter} />);
    expect(await screen.findByText("已连接")).toBeVisible();
    expect(screen.getByText("PowerShell")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "新建终端" }));
    await waitFor(() => expect(adapter.start).toHaveBeenCalledTimes(2));
    expect(screen.getAllByRole("button", { name: /关闭/ })).toHaveLength(2);
    fireEvent.click(screen.getByRole("button", { name: "中断" }));
    await waitFor(() => expect(adapter.interrupt).toHaveBeenCalledOnce());
  });

  it("does not open a terminal before Host readiness", () => {
    const adapter = port();
    render(<LocalTerminalWorkspace hostReady={false} ownerUserId="alice" port={adapter} />);
    expect(screen.getByText("本机终端暂不可用")).toBeVisible();
    expect(adapter.start).not.toHaveBeenCalled();
  });
});

function port(): LocalTerminalPort {
  const pending = new Promise<LocalTerminalPoll>(() => undefined);
  let session = 0;
  return {
    start: vi.fn(async () => {
      session += 1;
      return {
        sessionId: `local-terminal-${session}`,
        initialWorkingDirectory: "C:\\Users\\Alice",
        shellName: "PowerShell",
      };
    }),
    sendInput: vi.fn(async () => undefined),
    resize: vi.fn(async () => undefined),
    interrupt: vi.fn(async () => undefined),
    poll: vi.fn(() => pending),
    close: vi.fn(async () => undefined),
    discardPresentation: vi.fn(async () => undefined),
  };
}
