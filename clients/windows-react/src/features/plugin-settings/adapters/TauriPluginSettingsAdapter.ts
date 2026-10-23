import { invoke } from "@tauri-apps/api/core";
import { pluginSettingsSnapshotSchema } from "../domain/pluginSettings";
import type { PluginSettingsPort } from "../ports/PluginSettingsPort";

export class TauriPluginSettingsAdapter implements PluginSettingsPort {
  async load(ownerUserId: string) {
    return pluginSettingsSnapshotSchema.parse(await invoke("load_plugin_settings", { ownerUserId }));
  }

  async install(ownerUserId: string, pluginId: string) {
    return pluginSettingsSnapshotSchema.parse(await invoke("install_marketplace_plugin", {
      input: { ownerUserId, pluginId },
    }));
  }

  async updateEnabled(ownerUserId: string, pluginId: string, enabled: boolean) {
    return pluginSettingsSnapshotSchema.parse(await invoke("update_plugin_enabled", {
      input: { ownerUserId, pluginId, enabled },
    }));
  }

  async uninstall(ownerUserId: string, pluginId: string) {
    return pluginSettingsSnapshotSchema.parse(await invoke("uninstall_plugin", {
      input: { ownerUserId, pluginId },
    }));
  }
}
