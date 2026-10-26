import { z } from "zod";

export const pluginPermissionSchema = z.object({
  id: z.string().min(1),
  required: z.boolean(),
  reason: z.string().nullable(),
  status: z.literal("declared_not_checked"),
});

export const pluginCredentialSchema = z.object({
  name: z.string().min(1).max(128).regex(/^[A-Za-z0-9._-]+$/),
  configured: z.boolean(),
});

export const managedPluginSchema = z.object({
  id: z.string().min(1),
  packageName: z.string().nullable(),
  pluginKey: z.string().nullable(),
  displayName: z.string().min(1),
  description: z.string(),
  category: z.string().min(1),
  publisher: z.string().min(1),
  latestVersion: z.string().min(1),
  installedVersion: z.string().nullable(),
  installed: z.boolean(),
  updateAvailable: z.boolean(),
  enabled: z.boolean(),
  hasUi: z.boolean(),
  source: z.enum(["marketplace", "local"]),
  runtimeStatus: z.enum(["not_installed", "not_checked", "disabled"]),
  mcpServerCount: z.number().int().nonnegative(),
  skills: z.array(z.string().min(1)).max(128),
  permissions: z.array(pluginPermissionSchema).max(128),
  installationIssue: z.enum(["metadata_invalid", "integrity_check_failed"]).nullable(),
  windowsCompatible: z.boolean(),
  installActionAvailable: z.boolean(),
  credentials: z.array(pluginCredentialSchema).max(256),
});

export const pluginSettingsSnapshotSchema = z.object({
  plugins: z.array(managedPluginSchema),
  marketplaceAvailable: z.boolean(),
  localCatalogAvailable: z.boolean(),
  marketplaceError: z.string().nullable(),
  localCatalogError: z.string().nullable(),
  secureMarketplaceInstallAvailable: z.boolean(),
  manualInstallAvailable: z.boolean(),
});

export const manualPluginDirectorySelectionSchema = z.object({
  selectionId: z.string().min(1),
  directoryName: z.string().min(1),
});

export const manualPluginComponentSchema = z.object({
  kind: z.enum(["mcp", "skill"]),
  displayName: z.string().min(1),
});

export const manualPluginInspectionSchema = z.object({
  selectionId: z.string().min(1),
  directoryName: z.string().min(1),
  displayName: z.string().min(1),
  version: z.string().min(1),
  description: z.string(),
  fingerprint: z.string().regex(/^[a-f0-9]{64}$/),
  components: z.array(manualPluginComponentSchema).max(192),
});

export type ManagedPlugin = z.infer<typeof managedPluginSchema>;
export type PluginSettingsSnapshot = z.infer<typeof pluginSettingsSnapshotSchema>;
export type ManualPluginDirectorySelection = z.infer<typeof manualPluginDirectorySelectionSchema>;
export type ManualPluginInspection = z.infer<typeof manualPluginInspectionSchema>;

export function filterPlugins(plugins: ManagedPlugin[], queryInput: string) {
  const query = queryInput.trim().toLocaleLowerCase();
  if (!query) return plugins;
  return plugins.filter((plugin) => [
    plugin.displayName,
    plugin.description,
    plugin.publisher,
    plugin.category,
    plugin.packageName ?? "",
    plugin.pluginKey ?? "",
    ...plugin.skills,
  ].join(" ").toLocaleLowerCase().includes(query));
}
