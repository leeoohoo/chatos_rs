import { invoke } from "@tauri-apps/api/core";
import { deviceGatewaySnapshotSchema } from "../domain/deviceGateway";
import type { DeviceGatewayPort } from "../ports/DeviceGatewayPort";

export class TauriDeviceGatewayAdapter implements DeviceGatewayPort {
  async load(ownerUserId: string) {
    return deviceGatewaySnapshotSchema.parse(await invoke("load_device_gateway", { ownerUserId }));
  }
}
