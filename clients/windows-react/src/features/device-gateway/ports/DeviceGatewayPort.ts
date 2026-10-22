import type { DeviceGatewaySnapshot } from "../domain/deviceGateway";

export interface DeviceGatewayPort {
  load(ownerUserId: string): Promise<DeviceGatewaySnapshot>;
}
