import { z } from "zod";

const safeText = (maximum: number) => z.string().max(maximum).refine(
  (value) => !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u.test(value),
  "字段包含不安全的控制字符。",
);

export const remoteConnectionIdentifierSchema = safeText(1_000).min(1).refine(
  (value) => value.trim() === value,
  "远端连接标识不能包含首尾空白。",
);

const labelSchema = safeText(1_000).min(1).refine(
  (value) => value.trim() === value,
  "远端连接字段不能包含首尾空白。",
);

const optionalLabelSchema = labelSchema.nullable();
const portSchema = z.number().int().min(1).max(65_535);
const timestampSchema = z.number().int().nonnegative().safe();

export const remoteAuthenticationTypeSchema = z.enum([
  "private_key",
  "private_key_cert",
  "password",
]);

export const remoteHostKeyPolicySchema = z.enum(["strict", "accept_new"]);

export const remoteCredentialFileKindSchema = z.enum([
  "private_key",
  "certificate",
  "jump_private_key",
  "jump_certificate",
]);

export const remoteCredentialFileSelectionSchema = z.object({
  selectionId: remoteConnectionIdentifierSchema,
  fileName: safeText(1_024).min(1).refine(
    (value) => !value.includes("/") && !value.includes("\\"),
    "凭据文件名不能包含路径。",
  ),
  kind: remoteCredentialFileKindSchema,
}).strict();

export const remoteConnectionSchema = z.object({
  id: remoteConnectionIdentifierSchema,
  name: labelSchema,
  host: labelSchema,
  port: portSchema,
  username: labelSchema,
  authenticationType: remoteAuthenticationTypeSchema,
  hasPassword: z.boolean(),
  hasPrivateKeyPath: z.boolean(),
  hasCertificatePath: z.boolean(),
  defaultRemotePath: safeText(4_000).min(1).nullable(),
  hostKeyPolicy: remoteHostKeyPolicySchema,
  jumpEnabled: z.boolean(),
  jumpConnectionId: remoteConnectionIdentifierSchema.nullable(),
  jumpHost: optionalLabelSchema,
  jumpPort: portSchema.nullable(),
  jumpUsername: optionalLabelSchema,
  hasJumpPrivateKeyPath: z.boolean(),
  hasJumpCertificatePath: z.boolean(),
  hasJumpPassword: z.boolean(),
  lastActiveAtUnixMs: timestampSchema.nullable(),
  version: z.number().int().positive().safe(),
  createdAtUnixMs: timestampSchema,
  updatedAtUnixMs: timestampSchema,
}).strict().superRefine((connection, context) => {
  const hasManualJump = connection.jumpHost != null
    && connection.jumpPort != null
    && connection.jumpUsername != null;
  if (connection.jumpEnabled && connection.jumpConnectionId == null && !hasManualJump) {
    context.addIssue({ code: "custom", message: "启用跳板机的连接缺少有效路由。" });
  }
  if (!connection.jumpEnabled && (
    connection.jumpConnectionId != null
    || connection.jumpHost != null
    || connection.jumpPort != null
    || connection.jumpUsername != null
    || connection.hasJumpPrivateKeyPath
    || connection.hasJumpCertificatePath
    || connection.hasJumpPassword
  )) {
    context.addIssue({ code: "custom", message: "未启用跳板机的连接包含跳板机状态。" });
  }
  if (connection.updatedAtUnixMs < connection.createdAtUnixMs) {
    context.addIssue({ code: "custom", message: "远端连接更新时间早于创建时间。" });
  }
});

export const remoteConnectionListSchema = z.array(remoteConnectionSchema).max(10_000);

export const remoteSecretMutationSchema = z.discriminatedUnion("action", [
  z.object({ action: z.literal("keep") }).strict(),
  z.object({ action: z.literal("clear") }).strict(),
  z.object({ action: z.literal("replace"), value: z.string().min(1).max(1_024) }).strict(),
]);

export const remoteCredentialFileMutationSchema = z.discriminatedUnion("action", [
  z.object({ action: z.literal("keep") }).strict(),
  z.object({ action: z.literal("clear") }).strict(),
  z.object({
    action: z.literal("replace"),
    value: remoteConnectionIdentifierSchema.refine(
      (value) => value.startsWith("remote-credential-file-"),
      "SSH 凭据必须来自本机文件选择器。",
    ),
  }).strict(),
]);

export const remoteCredentialMutationSchema = z.object({
  password: remoteSecretMutationSchema,
  privateKeyPath: remoteCredentialFileMutationSchema,
  certificatePath: remoteCredentialFileMutationSchema,
  jumpPassword: remoteSecretMutationSchema,
  jumpPrivateKeyPath: remoteCredentialFileMutationSchema,
  jumpCertificatePath: remoteCredentialFileMutationSchema,
}).strict();

export const remoteConnectionInputSchema = z.object({
  name: safeText(1_000).min(1).nullable(),
  host: labelSchema,
  port: portSchema,
  username: labelSchema,
  authenticationType: remoteAuthenticationTypeSchema,
  defaultRemotePath: safeText(4_000).min(1).nullable(),
  hostKeyPolicy: remoteHostKeyPolicySchema,
  jumpEnabled: z.boolean(),
  jumpConnectionId: remoteConnectionIdentifierSchema.nullable(),
  jumpHost: optionalLabelSchema,
  jumpPort: portSchema.nullable(),
  jumpUsername: optionalLabelSchema,
  credentials: remoteCredentialMutationSchema,
}).strict().superRefine((input, context) => {
  const hasManualJump = input.jumpHost != null
    && input.jumpPort != null
    && input.jumpUsername != null;
  if (input.jumpEnabled && input.jumpConnectionId == null && !hasManualJump) {
    context.addIssue({ code: "custom", message: "启用跳板机后需要选择已有连接或填写完整配置。" });
  }
  if (!input.jumpEnabled && (
    input.jumpConnectionId != null
    || input.jumpHost != null
    || input.jumpPort != null
    || input.jumpUsername != null
  )) {
    context.addIssue({ code: "custom", message: "关闭跳板机后不能提交跳板机路由。" });
  }
});

export const remoteConnectionTestResultSchema = z.object({
  success: z.boolean(),
  verificationRequired: z.boolean(),
  message: safeText(64 * 1_024).min(1),
  verificationPrompt: safeText(64 * 1_024).min(1).nullable(),
}).strict().superRefine((result, context) => {
  if (result.success === result.verificationRequired) {
    context.addIssue({ code: "custom", message: "远端连接测试状态不一致。" });
  }
  if (result.verificationRequired !== (result.verificationPrompt != null)) {
    context.addIssue({ code: "custom", message: "远端连接验证提示状态不一致。" });
  }
});

export type RemoteConnection = z.infer<typeof remoteConnectionSchema>;
export type RemoteConnectionInput = z.infer<typeof remoteConnectionInputSchema>;
export type RemoteConnectionTestResult = z.infer<typeof remoteConnectionTestResultSchema>;
export type RemoteSecretMutation = z.infer<typeof remoteSecretMutationSchema>;
export type RemoteCredentialMutation = z.infer<typeof remoteCredentialMutationSchema>;
export type RemoteAuthenticationType = z.infer<typeof remoteAuthenticationTypeSchema>;
export type RemoteHostKeyPolicy = z.infer<typeof remoteHostKeyPolicySchema>;
export type RemoteCredentialFileKind = z.infer<typeof remoteCredentialFileKindSchema>;
export type RemoteCredentialFileSelection = z.infer<typeof remoteCredentialFileSelectionSchema>;

export function sortRemoteConnections(connections: RemoteConnection[]) {
  return [...connections].sort((left, right) => left.name.localeCompare(
    right.name,
    undefined,
    { sensitivity: "base", numeric: true },
  ));
}

export function keepRemoteCredentials() {
  const keep = { action: "keep" } as const;
  return {
    password: keep,
    privateKeyPath: keep,
    certificatePath: keep,
    jumpPassword: keep,
    jumpPrivateKeyPath: keep,
    jumpCertificatePath: keep,
  };
}
