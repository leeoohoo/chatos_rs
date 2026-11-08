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
          protocolVersion: 46,
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
    expect(screen.getByText("host_health_check: timeout")).toBeInTheDocument();
  });

  it("explains authentication readiness failures without exposing credentials", () => {
    render(
      <HostReadyGate
        snapshot={{
          phase: "degraded",
          ready: false,
          protocolVersion: 52,
          detail: "登录状态尚未准备好。",
          retryable: true,
        }}
        diagnostics={[{
          sequence: 1,
          timestampUnixMs: 1,
          event: "host_authentication",
          outcome: "unavailable",
        }]}
        onRetry={vi.fn()}
      />,
    );
    expect(screen.getByText("登录会话尚未准备好，请重新登录后重试本地服务。")).toBeInTheDocument();
    expect(screen.queryByText(/token|secret|password/i)).not.toBeInTheDocument();
  });
});
