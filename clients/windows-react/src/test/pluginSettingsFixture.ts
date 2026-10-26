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
      installActionAvailable: true,
      credentials: [],
    }],
    marketplaceAvailable: true,
    localCatalogAvailable: true,
    marketplaceError: null,
    localCatalogError: null,
    secureMarketplaceInstallAvailable: true,
    manualInstallAvailable: true,
  };
}

export function pluginSettingsPort(overrides: Partial<PluginSettingsPort> = {}): PluginSettingsPort {
  return {
    load: async () => pluginSettingsSnapshot(),
    install: async () => ({
      ...pluginSettingsSnapshot(),
      plugins: pluginSettingsSnapshot().plugins.map((plugin) => ({
        ...plugin,
        installed: true,
        installedVersion: plugin.latestVersion,
        updateAvailable: false,
      })),
    }),
    updateEnabled: async (_ownerUserId, _pluginId, enabled) => ({
      ...pluginSettingsSnapshot(),
      plugins: pluginSettingsSnapshot().plugins.map((plugin) => ({ ...plugin, enabled })),
    }),
    uninstall: async () => ({
      ...pluginSettingsSnapshot(),
      plugins: pluginSettingsSnapshot().plugins.map((plugin) => ({ ...plugin, installed: false, installedVersion: null })),
    }),
    updateCredentials: async (_ownerUserId, _pluginId, credentials) => ({
      ...pluginSettingsSnapshot(),
      plugins: pluginSettingsSnapshot().plugins.map((plugin) => ({
        ...plugin,
        credentials: Object.keys(credentials).map((name) => ({ name, configured: true })),
      })),
    }),
    pickManualDirectory: async () => ({ selectionId: "selection-1", directoryName: "my-plugin" }),
    inspectManualDirectory: async (_ownerUserId, _presentationId, selectionId) => ({
      selectionId,
      directoryName: "my-plugin",
      displayName: "My Local Plugin",
      version: "1.0.0",
      description: "A verified local plugin.",
      fingerprint: "a".repeat(64),
      components: [
        { kind: "mcp", displayName: "local-mcp" },
        { kind: "skill", displayName: "local-skill" },
      ],
    }),
    installManualDirectory: async () => pluginSettingsSnapshot(),
    discardManualDirectory: async () => undefined,
    ...overrides,
  };
}
