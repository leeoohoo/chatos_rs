import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { HostReadyGate } from "./HostReadyGate";

describe("HostReadyGate", () => {
  it("shows an explicit recoverable failure instead of an endless spinner", () => {
    const retry = vi.fn();
    render(
      <HostReadyGate
        snapshot={{
          phase: "degraded",
          ready: false,
          protocolVersion: 44,
          detail: "本地服务没有响应。",
          retryable: true,
        }}
        diagnostics={[{
          sequence: 1,
          timestampUnixMs: 1,
          event: "host_health_check",
          outcome: "timeout",
        }]}
        onRetry={retry}
      />,
    );
    expect(screen.getByText("本地服务没有响应。")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重新检查" }));
    expect(retry).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByText("诊断记录"));
    expect(screen.getByText("host_health_check")).toBeInTheDocument();
    expect(screen.getByText("timeout")).toBeInTheDocument();
  });
});
