import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { deviceGatewayPort } from "../../../test/deviceGatewayFixture";
import { useDeviceGatewayStore } from "../application/deviceGatewayStore";
import type { HostLifecyclePort } from "../../host-lifecycle/ports/HostLifecyclePort";
import { useHostLifecycleStore } from "../../host-lifecycle/application/hostLifecycleStore";
import { DeviceGatewayView } from "./DeviceGatewayView";

describe("DeviceGatewayView", () => {
  beforeEach(() => {
    useDeviceGatewayStore.getState().reset();
    useHostLifecycleStore.setState({
      ownerUserId: "owner-1",
      snapshot: { phase: "ready", ready: true, protocolVersion: 44, detail: "ready", retryable: false, label: "本地服务已就绪" },
    });
  });

  it("shows the real Windows connection model and local boundary projection", async () => {
    render(<DeviceGatewayView ownerUserId="owner-1" username="alice" port={deviceGatewayPort()} hostPort={hostPort()} />);
    expect(await screen.findByText("连接状态")).toBeInTheDocument();
    expect(screen.getByText("账号认证控制面")).toBeInTheDocument();
    expect(screen.getByText(/不宣称具备 macOS/)).toBeInTheDocument();
    expect(screen.getByText("2 个工作区")).toBeInTheDocument();
    expect(screen.queryByText(/private|fingerprint/i)).not.toBeInTheDocument();
  });
});

function hostPort(): HostLifecyclePort {
  const ready = { phase: "ready" as const, ready: true, protocolVersion: 44, detail: "ready", retryable: false };
  return {
    snapshot: vi.fn(async () => ready),
    diagnostics: vi.fn(async () => ({ capacity: 64, entries: [] })),
    start: vi.fn(async () => ready),
    stop: vi.fn(async () => ({ ...ready, phase: "stopped" as const, ready: false })),
  };
}
