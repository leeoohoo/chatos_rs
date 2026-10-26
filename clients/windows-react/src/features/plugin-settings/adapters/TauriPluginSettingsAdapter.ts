import { invoke } from "@tauri-apps/api/core";
import { manualPluginDirectorySelectionSchema, manualPluginInspectionSchema, pluginSettingsSnapshotSchema } from "../domain/pluginSettings";
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

  async pickManualDirectory(ownerUserId: string, presentationId: string) {
    const result = await invoke("pick_manual_plugin_directory", { ownerUserId, presentationId });
    return result == null ? null : manualPluginDirectorySelectionSchema.parse(result);
  }

  async inspectManualDirectory(ownerUserId: string, presentationId: string, selectionId: string) {
    return manualPluginInspectionSchema.parse(await invoke("inspect_manual_plugin_directory", {
      input: { ownerUserId, presentationId, selectionId },
    }));
  }

  async installManualDirectory(ownerUserId: string, presentationId: string, selectionId: string, expectedFingerprint: string) {
    return pluginSettingsSnapshotSchema.parse(await invoke("install_manual_plugin_directory", {
      input: { ownerUserId, presentationId, selectionId, expectedFingerprint },
    }));
  }

  async discardManualDirectory(ownerUserId: string, presentationId: string) {
    await invoke("discard_manual_plugin_directory", { ownerUserId, presentationId });
  }
}
