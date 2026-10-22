import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DeviceGatewaySnapshot } from "../domain/deviceGateway";
import { deviceGatewayPort, deviceGatewaySnapshot } from "../../../test/deviceGatewayFixture";
import { useDeviceGatewayStore } from "./deviceGatewayStore";

describe("device gateway store", () => {
  beforeEach(() => useDeviceGatewayStore.getState().reset());

  it("loads owner-scoped native status", async () => {
    const port = deviceGatewayPort();
    await useDeviceGatewayStore.getState().load(port, "owner-1");
    expect(port.load).toHaveBeenCalledWith("owner-1");
    expect(useDeviceGatewayStore.getState().snapshot?.workspaceCount).toBe(2);
  });

  it("rejects a late status after teardown", async () => {
    let release!: (value: DeviceGatewaySnapshot) => void;
    const port = deviceGatewayPort({ load: vi.fn(async () => new Promise<DeviceGatewaySnapshot>((resolve) => { release = resolve; })) });
    const pending = useDeviceGatewayStore.getState().load(port, "owner-1");
    useDeviceGatewayStore.getState().reset();
    release(deviceGatewaySnapshot());
    await pending;
    expect(useDeviceGatewayStore.getState().snapshot).toBeNull();
  });
});
