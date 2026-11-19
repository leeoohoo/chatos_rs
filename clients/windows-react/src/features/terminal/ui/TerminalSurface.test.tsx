import { cleanup, render } from "@testing-library/react";
import { createRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TerminalSurface, type TerminalSurfaceHandle } from "./TerminalSurface";

const terminalCalls = vi.hoisted(() => ({
  clearSelection: vi.fn(),
}));
const searchCalls = vi.hoisted(() => ({
  clearDecorations: vi.fn(),
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit() {}
  },
}));

vi.mock("@xterm/addon-search", () => ({
  SearchAddon: class {
    clearDecorations() { searchCalls.clearDecorations(); }
    findNext() { return false; }
    findPrevious() { return false; }
  },
}));

vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    parser = { registerOscHandler: () => disposable() };
    clear() {}
    clearSelection() { terminalCalls.clearSelection(); }
    dispose() {}
    focus() {}
    loadAddon() {}
    onBinary() { return disposable(); }
    onData() { return disposable(); }
    onResize() { return disposable(); }
    onTitleChange() { return disposable(); }
    open() {}
    reset() {}
    write() {}
  },
}));

beforeEach(() => {
  terminalCalls.clearSelection.mockReset();
  searchCalls.clearDecorations.mockReset();
});

afterEach(() => cleanup());

describe("TerminalSurface", () => {
  it("clears both search decorations and the active xterm selection", () => {
    const ref = createRef<TerminalSurfaceHandle>();
    render(
      <TerminalSurface
        ariaLabel="终端输出"
        className="terminal"
        events={[]}
        lifecycleKey="session"
        onInput={() => undefined}
        onOsc7={() => undefined}
        onResize={() => undefined}
        onTitleChange={() => undefined}
        outputGeneration={0}
        ref={ref}
      />,
    );

    ref.current?.clearSearch();

    expect(searchCalls.clearDecorations).toHaveBeenCalledOnce();
    expect(terminalCalls.clearSelection).toHaveBeenCalledOnce();
  });
});

function disposable() {
  return { dispose() {} };
}
