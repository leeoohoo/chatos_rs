import { vi } from "vitest";
import type { DeviceGatewaySnapshot } from "../features/device-gateway/domain/deviceGateway";
import type { DeviceGatewayPort } from "../features/device-gateway/ports/DeviceGatewayPort";

export function deviceGatewaySnapshot(): DeviceGatewaySnapshot {
  return {
    configured: true,
    connectorRunning: true,
    deviceName: "Windows PC",
    deviceId: "windows-device-1",
    cloudBaseUrl: "https://gateway.example.com",
    workspaceCount: 2,
    runtime: "Rust Local Agent Host",
    developerMode: false,
    connectionMode: "authenticated_control_plane",
    deviceSignedRelayAvailable: false,
  };
}

export function deviceGatewayPort(overrides: Partial<DeviceGatewayPort> = {}): DeviceGatewayPort {
  return { load: vi.fn(async () => deviceGatewaySnapshot()), ...overrides };
}
