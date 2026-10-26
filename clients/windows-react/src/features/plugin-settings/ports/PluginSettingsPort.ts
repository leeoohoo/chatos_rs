import type { ManualPluginDirectorySelection, ManualPluginInspection, PluginSettingsSnapshot } from "../domain/pluginSettings";

export interface PluginSettingsPort {
  load(ownerUserId: string): Promise<PluginSettingsSnapshot>;
  install(ownerUserId: string, pluginId: string): Promise<PluginSettingsSnapshot>;
  updateEnabled(ownerUserId: string, pluginId: string, enabled: boolean): Promise<PluginSettingsSnapshot>;
  uninstall(ownerUserId: string, pluginId: string): Promise<PluginSettingsSnapshot>;
  pickManualDirectory(ownerUserId: string, presentationId: string): Promise<ManualPluginDirectorySelection | null>;
  inspectManualDirectory(ownerUserId: string, presentationId: string, selectionId: string): Promise<ManualPluginInspection>;
  installManualDirectory(ownerUserId: string, presentationId: string, selectionId: string, expectedFingerprint: string): Promise<PluginSettingsSnapshot>;
  discardManualDirectory(ownerUserId: string, presentationId: string): Promise<void>;
}
