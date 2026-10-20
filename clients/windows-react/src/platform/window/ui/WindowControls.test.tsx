import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { WindowChromePort } from "../ports/WindowChromePort";
import { WindowControls } from "./WindowControls";

describe("WindowControls", () => {
  it("routes each window action through the platform port", () => {
    const windowChrome: WindowChromePort = {
      close: vi.fn().mockResolvedValue(undefined),
      minimize: vi.fn().mockResolvedValue(undefined),
      toggleMaximize: vi.fn().mockResolvedValue(undefined),
    };
    render(<WindowControls windowChrome={windowChrome} />);

    fireEvent.click(screen.getByRole("button", { name: "关闭窗口" }));
    fireEvent.click(screen.getByRole("button", { name: "最小化窗口" }));
    fireEvent.click(screen.getByRole("button", { name: "最大化或还原窗口" }));

    expect(windowChrome.close).toHaveBeenCalledOnce();
    expect(windowChrome.minimize).toHaveBeenCalledOnce();
    expect(windowChrome.toggleMaximize).toHaveBeenCalledOnce();
  });
});
