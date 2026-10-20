import { z } from "zod";

export const systemPermissionStatusSchema = z.enum([
  "ready",
  "not_applicable",
  "on_demand",
  "needs_attention",
  "missing_dependency",
  "action_required",
]);

export const systemPermissionSchema = z.object({
  id: z.string().trim().min(1).max(1_024),
  label: z.string().trim().min(1).max(500),
  summary: z.string().trim().min(1).max(2_000),
  status: systemPermissionStatusSchema,
  statusLabel: z.string().trim().min(1).max(500),
  required: z.boolean(),
  canRequest: z.boolean(),
  requestLabel: z.string().trim().min(1).max(500),
  note: z.string().max(4_000),
  lastError: z.string().trim().min(1).max(4_000).nullable(),
}).strict();

export const systemPermissionsSchema = z.object({
  platform: z.string().trim().min(1).max(100),
  platformLabel: z.string().trim().min(1).max(200),
  items: z.array(systemPermissionSchema).max(32),
}).strict().superRefine((snapshot, context) => {
  const identifiers = new Set<string>();
  snapshot.items.forEach((permission, index) => {
    if (identifiers.has(permission.id)) {
      context.addIssue({
        code: "custom",
        message: "系统权限标识不能重复。",
        path: ["items", index, "id"],
      });
    }
    identifiers.add(permission.id);
  });
});

export type SystemPermissionStatus = z.infer<typeof systemPermissionStatusSchema>;
export type SystemPermission = z.infer<typeof systemPermissionSchema>;
export type SystemPermissions = z.infer<typeof systemPermissionsSchema>;

export function isPermissionReady(status: SystemPermissionStatus) {
  return status === "ready" || status === "not_applicable" || status === "on_demand";
}

export function permissionTone(status: SystemPermissionStatus): "ready" | "attention" | "neutral" {
  if (isPermissionReady(status)) return "ready";
  if (status === "needs_attention" || status === "missing_dependency" || status === "action_required") {
    return "attention";
  }
  return "neutral";
}
