import { invoke } from "@tauri-apps/api/core";
import {
  pluginApplicationBridgeResponseSchema,
  pluginApplicationLaunchSchema,
  pluginApplicationListSchema,
} from "../domain/pluginApplications";
import type { PluginApplicationPort } from "../ports/PluginApplicationPort";

export class TauriPluginApplicationAdapter implements PluginApplicationPort {
  async list(ownerUserId: string) {
    return pluginApplicationListSchema.parse(
      await invoke("list_plugin_applications", { ownerUserId }),
    );
  }

  async launch(request: Parameters<PluginApplicationPort["launch"]>[0]) {
    return pluginApplicationLaunchSchema.parse(
      await invoke("launch_plugin_application", { request }),
    );
  }

  async discardPresentation(
    request: Parameters<PluginApplicationPort["discardPresentation"]>[0],
  ) {
    await invoke("discard_plugin_application_presentation", { ...request });
  }

  async handleBridgeRequest(
    request: Parameters<PluginApplicationPort["handleBridgeRequest"]>[0],
  ) {
    return pluginApplicationBridgeResponseSchema.parse(
      await invoke("handle_plugin_application_bridge_request", { request }),
    );
  }
}
