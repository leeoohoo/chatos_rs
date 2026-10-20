import { z } from "zod";

export const accessControlBackendSchema = z.object({
  backend: z.string().trim().min(1).max(200),
  label: z.string().trim().min(1).max(500),
  status: z.enum(["ready", "limited", "unavailable"]),
  statusLabel: z.string().trim().min(1).max(500),
  selectable: z.boolean(),
  filesystemIsolation: z.boolean(),
  networkIsolation: z.boolean(),
  processTreeControl: z.boolean(),
  message: z.string().trim().min(1).max(4_000),
}).strict();

export const accessControlSettingsSchema = z.object({
  enabled: z.boolean(),
  editable: z.boolean(),
  defaultBackend: z.string().trim().min(1).max(200),
  defaultPermissionProfileId: z.string().trim().min(1).max(200),
  defaultPermissionProfileName: z.string().trim().min(1).max(500),
  defaultApprovalPolicy: z.enum(["on_request", "never"]),
  defaultApprovalReviewer: z.enum(["user", "auto_review", "policy"]),
  defaultNetworkAccess: z.enum(["disabled", "controlled", "host"]),
  permissionConfigurationError: z.string().trim().min(1).max(4_000).nullable(),
  policyRevision: z.string().trim().min(1).max(500),
}).strict();

export const accessControlSnapshotSchema = z.object({
  settings: accessControlSettingsSchema,
  backends: z.array(accessControlBackendSchema).min(1).max(16),
}).strict().superRefine((snapshot, context) => {
  const identifiers = new Set<string>();
  snapshot.backends.forEach((backend, index) => {
    if (identifiers.has(backend.backend)) {
      context.addIssue({
        code: "custom",
        message: "访问控制后端标识不能重复。",
        path: ["backends", index, "backend"],
      });
    }
    identifiers.add(backend.backend);
  });
  if (!identifiers.has(snapshot.settings.defaultBackend)) {
    context.addIssue({
      code: "custom",
      message: "默认访问控制后端必须出现在后端列表中。",
      path: ["settings", "defaultBackend"],
    });
  }
});

export type AccessControlBackend = z.infer<typeof accessControlBackendSchema>;
export type AccessControlSnapshot = z.infer<typeof accessControlSnapshotSchema>;

export function approvalPolicyLabel(snapshot: AccessControlSnapshot) {
  const { defaultApprovalPolicy: policy, defaultApprovalReviewer: reviewer } = snapshot.settings;
  if (policy === "never") return "完全控制（不逐次询问）";
  if (reviewer === "auto_review") return "按需审批 · 审批模型";
  return "按需审批 · 用户";
}

export function networkAccessLabel(access: AccessControlSnapshot["settings"]["defaultNetworkAccess"]) {
  if (access === "disabled") return "禁用";
  if (access === "controlled") return "受控访问";
  return "跟随 Windows 本机";
}
