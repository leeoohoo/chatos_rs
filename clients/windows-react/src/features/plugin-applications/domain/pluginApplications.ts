import { z } from "zod";

const safeText = (maximum: number) => z.string().max(maximum).refine(
  (value) => !/[\u0000-\u001f\u007f]/u.test(value),
  "字段包含不安全的控制字符。",
);

const identifierSchema = safeText(1_024).min(1).refine(
  (value) => value.trim() === value,
  "标识不能包含首尾空白。",
);

const releaseDigestSchema = z.string().refine((value) => {
  const digest = value.startsWith("sha256:") ? value.slice(7) : value;
  return /^[0-9a-fA-F]{64}$/u.test(digest);
}, "插件摘要不是有效的 SHA-256。");

const bridgeCapabilitySchema = z.string()
  .min(1)
  .max(256)
  .regex(/^[A-Za-z0-9._-]+$/u);

export const pluginApplicationSchema = z.object({
  pluginId: identifierSchema,
  componentKey: identifierSchema,
  displayName: safeText(1_024).min(1),
  description: safeText(16 * 1_024),
  brandColor: z.string().regex(/^#[0-9a-fA-F]{6}$/u).nullable(),
  requiresLocalRuntime: z.boolean(),
  contextScope: z.enum(["device", "workspace", "project"]).nullable(),
  missingContext: z.enum(["device", "reject"]).nullable(),
  bridgeCapabilities: z.array(bridgeCapabilitySchema).max(64),
  releaseId: identifierSchema,
  releaseDigest: releaseDigestSchema,
}).strict().superRefine((application, context) => {
  if ((application.contextScope == null) !== (application.missingContext == null)) {
    context.addIssue({ code: "custom", message: "插件应用上下文声明不完整。" });
  }
  if (new Set(application.bridgeCapabilities).size !== application.bridgeCapabilities.length) {
    context.addIssue({ code: "custom", message: "插件应用宿主能力重复。" });
  }
});

export const pluginApplicationListSchema = z.array(pluginApplicationSchema).max(1_000);

export const pluginApplicationContextSelectionSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("none") }).strict(),
  z.object({ kind: z.literal("device") }).strict(),
  z.object({ kind: z.literal("project"), projectId: identifierSchema }).strict(),
]);

export const pluginApplicationLaunchSchema = z.object({
  sessionId: identifierSchema,
  hostSessionNonce: z.string().length(64).regex(/^[0-9a-f]+$/u),
  application: pluginApplicationSchema,
  url: z.string().url(),
  bridgeOrigin: z.string().url(),
  bridgeCapabilities: z.array(bridgeCapabilitySchema).max(64),
  storagePartitionId: identifierSchema,
  context: z.object({
    projectId: identifierSchema,
    projectName: safeText(1_024).min(1),
  }).strict().nullable(),
  releaseId: identifierSchema,
  releaseDigest: releaseDigestSchema,
  version: safeText(1_024).min(1),
}).strict().superRefine((launch, context) => {
  const url = parseLoopbackUrl(launch.url);
  const origin = parseLoopbackOrigin(launch.bridgeOrigin);
  if (!url || !origin || url.origin !== origin.origin) {
    context.addIssue({ code: "custom", message: "插件应用启动地址不是同源 loopback HTTP 地址。" });
  }
  if (applicationId(launch.application) === ":") {
    context.addIssue({ code: "custom", message: "插件应用标识无效。" });
  }
  if (launch.releaseId !== launch.application.releaseId
    || launch.releaseDigest !== launch.application.releaseDigest) {
    context.addIssue({ code: "custom", message: "插件应用启动版本与目录记录不一致。" });
  }
  if (launch.bridgeCapabilities.some(
    (capability) => !launch.application.bridgeCapabilities.includes(capability),
  )) {
    context.addIssue({ code: "custom", message: "插件应用启动结果包含未声明的宿主能力。" });
  }
});

export const pluginApplicationBridgeResponseSchema = z.object({
  requestId: identifierSchema.max(256),
  result: z.unknown(),
}).strict().refine(
  (response) => Object.prototype.hasOwnProperty.call(response, "result"),
  "插件宿主响应缺少结果。",
);

export const pluginFrameRequestSchema = z.object({
  type: z.literal("chatos.plugin_ui.request"),
  protocol_version: z.literal(1),
  adapter_session_id: identifierSchema,
  host_session_nonce: z.string().length(64).regex(/^[0-9a-f]+$/u),
  request_id: identifierSchema.max(256),
  method: bridgeCapabilitySchema,
  payload: z.record(z.string(), z.unknown()),
}).strict();

export type PluginApplication = z.infer<typeof pluginApplicationSchema>;
export type PluginApplicationContextSelection = z.infer<typeof pluginApplicationContextSelectionSchema>;
export type PluginApplicationLaunch = z.infer<typeof pluginApplicationLaunchSchema>;
export type PluginFrameRequest = z.infer<typeof pluginFrameRequestSchema>;

export function applicationId(application: Pick<PluginApplication, "pluginId" | "componentKey">): string {
  return `${application.pluginId}:${application.componentKey}`;
}

export function requiresPluginProjectContext(application: PluginApplication): boolean {
  return application.contextScope === "project" || application.contextScope === "workspace";
}

function parseLoopbackUrl(value: string): URL | null {
  try {
    const url = new URL(value);
    return isRestrictedLoopback(url) ? url : null;
  } catch {
    return null;
  }
}

function parseLoopbackOrigin(value: string): URL | null {
  const url = parseLoopbackUrl(value);
  return url && url.pathname === "/" && url.search === "" && url.hash === "" ? url : null;
}

function isRestrictedLoopback(url: URL): boolean {
  return url.protocol === "http:"
    && url.hostname === "127.0.0.1"
    && url.port !== ""
    && url.username === ""
    && url.password === ""
    && url.hash === "";
}
