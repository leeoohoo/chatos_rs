import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { deviceGatewayPort } from "../../../test/deviceGatewayFixture";
import { useDeviceGatewayStore } from "../application/deviceGatewayStore";
import type { HostLifecyclePort } from "../../host-lifecycle/ports/HostLifecyclePort";
import { useHostLifecycleStore } from "../../host-lifecycle/application/hostLifecycleStore";
import { DeviceGatewayView } from "./DeviceGatewayView";

describe("DeviceGatewayView", () => {
  afterEach(() => cleanup());
  beforeEach(() => {
    useDeviceGatewayStore.getState().reset();
    useHostLifecycleStore.setState({
      ownerUserId: "owner-1",
      snapshot: { phase: "ready", ready: true, protocolVersion: 46, detail: "ready", retryable: false, label: "本地服务已就绪" },
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

  it("surfaces Host recovery detail and bounded diagnostics after a failed start", async () => {
    useHostLifecycleStore.setState({
      snapshot: {
        phase: "degraded",
        ready: false,
        protocolVersion: 46,
        detail: "Local Agent Host executable is unavailable.",
        retryable: true,
        label: "本地服务需要恢复",
      },
      diagnostics: [{
        sequence: 1,
        timestampUnixMs: 1,
        event: "host_executable",
        outcome: "unavailable",
      }],
    });
    render(<DeviceGatewayView ownerUserId="owner-1" username="alice" port={deviceGatewayPort()} hostPort={hostPort()} />);
    expect((await screen.findByRole("alert")).textContent).toContain("Local Agent Host executable is unavailable.");
    expect(screen.getByText("诊断记录")).toBeInTheDocument();
    expect(screen.getByText("host_executable: unavailable")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重试本机服务" })).toBeInTheDocument();
  });
});

function hostPort(): HostLifecyclePort {
  const ready = { phase: "ready" as const, ready: true, protocolVersion: 46, detail: "ready", retryable: false };
  return {
    snapshot: vi.fn(async () => ready),
    diagnostics: vi.fn(async () => ({ capacity: 64, entries: [] })),
    resourceSnapshot: vi.fn(async () => ({
      poolId: "windows-machine-resource-units",
      configuredResourceUnits: 2,
      availableResourceUnits: 2,
      maximumBackgroundResourceUnits: 2,
      usedResourceUnits: 0,
      queuedCount: 0,
      pressureLevel: "normal" as const,
      domains: [
        { domain: "mediaWorker" as const, configuredLimit: 2, activeCount: 0, queuedCount: 0 },
        { domain: "codeReadWorker" as const, configuredLimit: 2, activeCount: 0, queuedCount: 0 },
      ],
      host: null,
    })),
    start: vi.fn(async () => ready),
    stop: vi.fn(async () => ({ ...ready, phase: "stopped" as const, ready: false })),
  };
}
