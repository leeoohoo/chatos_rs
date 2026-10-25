import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { forwardRef, useImperativeHandle } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LocalTerminalPoll } from "../domain/localTerminal";
import type { LocalTerminalPort } from "../ports/LocalTerminalPort";
import { useLocalTerminalStore } from "../application/localTerminalStore";
import { LocalTerminalWorkspace } from "./LocalTerminalWorkspace";

const surfaceHandle = vi.hoisted(() => ({
  clear: vi.fn(),
  clearSearch: vi.fn(),
  findNext: vi.fn(),
  findPrevious: vi.fn(),
  focus: vi.fn(),
}));

vi.mock("./LocalTerminalSurface", () => ({
  LocalTerminalSurface: forwardRef(function MockSurface(_props, ref) {
    useImperativeHandle(ref, () => surfaceHandle);
    return <div role="application" aria-label="本机终端输出" />;
  }),
}));

beforeEach(() => {
  Object.values(surfaceHandle).forEach((method) => method.mockReset());
  useLocalTerminalStore.setState({
    ownerUserId: null,
    hostAvailable: false,
    generation: useLocalTerminalStore.getState().generation + 1,
    tabs: [],
    selectedTabId: null,
  });
});
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
    fireEvent.click(screen.getByRole("button", { name: "终端操作" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /中断/ }));
    await waitFor(() => expect(adapter.interrupt).toHaveBeenCalledOnce());
  });

  it("searches incrementally and keeps clear and close actions in the macOS-style menu", async () => {
    const adapter = port();
    render(<LocalTerminalWorkspace hostReady ownerUserId="alice" port={adapter} />);
    await screen.findByText("已连接");

    fireEvent.change(screen.getByRole("textbox", { name: "搜索终端输出" }), {
      target: { value: "needle" },
    });
    await waitFor(() => expect(surfaceHandle.findNext).toHaveBeenCalledWith("needle"));
    fireEvent.click(screen.getByRole("button", { name: "上一个匹配项" }));
    expect(surfaceHandle.findPrevious).toHaveBeenCalledWith("needle");

    fireEvent.click(screen.getByRole("button", { name: "终端操作" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /清屏/ }));
    expect(surfaceHandle.clear).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "终端操作" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /关闭终端/ }));
    await waitFor(() => expect(adapter.close).toHaveBeenCalledOnce());
  });

  it("does not impose the Windows-only four-tab limit", async () => {
    const adapter = port();
    render(<LocalTerminalWorkspace hostReady ownerUserId="alice" port={adapter} />);
    await screen.findByText("已连接");
    for (let index = 0; index < 5; index += 1) {
      fireEvent.click(screen.getByRole("button", { name: "新建终端" }));
    }
    await waitFor(() => expect(adapter.start).toHaveBeenCalledTimes(6));
    expect(screen.getAllByRole("button", { name: /^关闭 / })).toHaveLength(6);
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
