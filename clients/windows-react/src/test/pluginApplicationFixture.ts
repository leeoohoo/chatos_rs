import type {
  PluginApplication,
  PluginApplicationLaunch,
} from "../features/plugin-applications/domain/pluginApplications";

const digest = "a".repeat(64);

export function pluginApplicationFixture(
  overrides: Partial<PluginApplication> = {},
): PluginApplication {
  return {
    pluginId: "com.example.canvas",
    componentKey: "canvas-workbench",
    displayName: "Canvas",
    description: "在独立工作区中绘制图形。",
    brandColor: "#0A84FF",
    requiresLocalRuntime: true,
    contextScope: "project",
    missingContext: "device",
    bridgeCapabilities: ["host.context.read"],
    releaseId: "release-1",
    releaseDigest: digest,
    ...overrides,
  };
}

export function pluginApplicationLaunchFixture(
  overrides: Partial<PluginApplicationLaunch> = {},
): PluginApplicationLaunch {
  const application = overrides.application ?? pluginApplicationFixture();
  return {
    sessionId: "plugin-session-1",
    hostSessionNonce: "b".repeat(64),
    application,
    url: "http://127.0.0.1:43127/",
    bridgeOrigin: "http://127.0.0.1:43127",
    bridgeCapabilities: ["host.context.read"],
    storagePartitionId: "plugin-storage-1",
    context: { projectId: "project-1", projectName: "Project One" },
    releaseId: application.releaseId,
    releaseDigest: application.releaseDigest,
    version: "1.0.0",
    ...overrides,
  };
}
