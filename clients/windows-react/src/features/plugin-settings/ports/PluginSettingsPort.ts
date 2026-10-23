import type { PluginSettingsSnapshot } from "../domain/pluginSettings";

export interface PluginSettingsPort {
  load(ownerUserId: string): Promise<PluginSettingsSnapshot>;
  install(ownerUserId: string, pluginId: string): Promise<PluginSettingsSnapshot>;
  updateEnabled(ownerUserId: string, pluginId: string, enabled: boolean): Promise<PluginSettingsSnapshot>;
  uninstall(ownerUserId: string, pluginId: string): Promise<PluginSettingsSnapshot>;
}
