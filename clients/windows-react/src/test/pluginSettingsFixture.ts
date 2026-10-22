import type { PluginSettingsSnapshot } from "../features/plugin-settings/domain/pluginSettings";
import type { PluginSettingsPort } from "../features/plugin-settings/ports/PluginSettingsPort";

export function pluginSettingsSnapshot(): PluginSettingsSnapshot {
  return {
    plugins: [{
      id: "plugin-1",
      packageName: "@chatos/diagram",
      pluginKey: "diagram",
      displayName: "Diagram Studio",
      description: "Create editable diagrams.",
      category: "Productivity",
      publisher: "ChatOS",
      latestVersion: "1.2.0",
      installedVersion: "1.1.0",
      installed: true,
      updateAvailable: true,
      enabled: true,
      hasUi: true,
      source: "marketplace",
      runtimeStatus: "not_checked",
      mcpServerCount: 1,
      skills: ["diagram-architecture", "diagram-flowchart"],
      permissions: [{ id: "process.spawn", required: true, reason: "Launch MCP", status: "declared_not_checked" }],
      installationIssue: null,
      windowsCompatible: true,
      installActionAvailable: false,
    }],
    marketplaceAvailable: true,
    localCatalogAvailable: true,
    marketplaceError: null,
    localCatalogError: null,
    secureMarketplaceInstallAvailable: false,
    manualInstallAvailable: false,
  };
}

export function pluginSettingsPort(overrides: Partial<PluginSettingsPort> = {}): PluginSettingsPort {
  return {
    load: async () => pluginSettingsSnapshot(),
    updateEnabled: async (_ownerUserId, _pluginId, enabled) => ({
      ...pluginSettingsSnapshot(),
      plugins: pluginSettingsSnapshot().plugins.map((plugin) => ({ ...plugin, enabled })),
    }),
    uninstall: async () => ({
      ...pluginSettingsSnapshot(),
      plugins: pluginSettingsSnapshot().plugins.map((plugin) => ({ ...plugin, installed: false, installedVersion: null })),
    }),
    ...overrides,
  };
}
