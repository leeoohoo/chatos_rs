import { z } from "zod";

const safeText = (maximum: number) => z.string().max(maximum).refine(
  (value) => !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u.test(value),
  "字段包含不安全的控制字符。",
);

const identifierSchema = safeText(1_024).min(1).refine(
  (value) => value.trim() === value,
  "标识不能包含首尾空白。",
);

const relativePathSchema = safeText(16 * 1_024).min(1).refine(
  (value) => value === "." || (
    !value.startsWith("/")
    && !value.includes("\\")
    && !value.includes(":")
    && value.split("/").every((component) => component.length > 0 && component !== "." && component !== "..")
  ),
  "路径必须是规范化的项目相对路径。",
);

export const projectRunTargetSchema = z.object({
  id: identifierSchema,
  label: safeText(16 * 1_024).min(1),
  kind: safeText(1_024).min(1),
  language: safeText(1_024).min(1).nullable(),
  cwd: relativePathSchema,
  command: safeText(64 * 1_024).min(1),
  source: safeText(16 * 1_024).min(1),
  isDefault: z.boolean(),
  entrypoint: relativePathSchema.nullable(),
  manifestPath: relativePathSchema.nullable(),
  requiredToolchains: z.array(identifierSchema).max(32),
});

export const projectRunCatalogSchema = z.object({
  projectId: identifierSchema,
  status: z.enum(["ready", "empty"]),
  defaultTargetId: identifierSchema.nullable(),
  targets: z.array(projectRunTargetSchema),
  errorMessage: safeText(64 * 1_024).min(1).nullable(),
}).superRefine((catalog, context) => {
  if ((catalog.status === "empty") !== (catalog.targets.length === 0)) {
    context.addIssue({ code: "custom", message: "运行目标目录状态不一致。" });
  }
  if (catalog.defaultTargetId != null
    && !catalog.targets.some((target) => target.id === catalog.defaultTargetId && target.isDefault)) {
    context.addIssue({ code: "custom", message: "默认运行目标不存在。" });
  }
  if (catalog.targets.filter((target) => target.isDefault).length > 1) {
    context.addIssue({ code: "custom", message: "运行目标包含多个默认项。" });
  }
});

export const projectRunToolchainOptionSchema = z.object({
  id: identifierSchema,
  kind: identifierSchema,
  label: safeText(16 * 1_024).min(1),
  version: safeText(16 * 1_024).min(1).nullable(),
  source: safeText(1_024).min(1),
  isDefault: z.boolean(),
});

export const projectRunConfigurationFileSchema = z.object({
  id: identifierSchema,
  kind: safeText(1_024).min(1),
  label: safeText(16 * 1_024).min(1),
  path: relativePathSchema,
  preview: z.string().max(2 * 1_024 * 1_024).nullable(),
  source: safeText(1_024).min(1),
});

export const projectRunValidationIssueSchema = z.object({
  id: identifierSchema,
  kind: z.enum(["warning", "error"]),
  message: safeText(64 * 1_024).min(1),
  targetId: identifierSchema.nullable(),
  targetLabel: safeText(16 * 1_024).min(1).nullable(),
  path: safeText(16 * 1_024).min(1).nullable(),
  hint: safeText(64 * 1_024).min(1).nullable(),
});

export const projectRunEnvironmentVariableSchema = z.object({
  key: z.string().min(1).max(128).regex(/^[A-Za-z_][A-Za-z0-9_]*$/),
  hasValue: z.boolean(),
});

export const projectRunEnvironmentSchema = z.object({
  toolchainOptions: z.record(identifierSchema, z.array(projectRunToolchainOptionSchema).max(128)),
  configurationFiles: z.array(projectRunConfigurationFileSchema),
  validationIssues: z.array(projectRunValidationIssueSchema),
  selectedToolchains: z.record(identifierSchema, identifierSchema),
  customToolchains: z.record(identifierSchema, z.object({
    kind: identifierSchema,
    label: safeText(16 * 1_024).min(1),
    executableId: identifierSchema,
  })),
  environmentVariables: z.array(projectRunEnvironmentVariableSchema).max(128),
  terminalUiEnabled: z.boolean(),
});

export const projectRunAnalysisSnapshotSchema = z.object({
  catalog: projectRunCatalogSchema,
  environment: projectRunEnvironmentSchema,
});

export const projectRunInstanceSchema = z.object({
  id: identifierSchema,
  targetId: identifierSchema,
  name: safeText(16 * 1_024).min(1),
  cwd: relativePathSchema,
  status: z.enum(["running", "stopping", "exited", "failed"]),
  isBusy: z.boolean(),
  isRunning: z.boolean(),
  log: z.string().max(512 * 1_024),
  logTruncated: z.boolean(),
  startedAtUnixMs: z.number().int().nonnegative().safe(),
  exitCode: z.number().int().nullable(),
}).superRefine((instance, context) => {
  if (instance.isRunning !== (["running", "stopping"] as string[]).includes(instance.status)) {
    context.addIssue({ code: "custom", message: "运行实例状态与运行标记不一致。" });
  }
  if (instance.isBusy !== (instance.status === "stopping")) {
    context.addIssue({ code: "custom", message: "运行实例状态与忙碌标记不一致。" });
  }
});

export const projectRunStateSchema = z.object({
  projectId: identifierSchema,
  status: z.enum(["running", "idle"]),
  isBusy: z.boolean(),
  isRunning: z.boolean(),
  revision: z.number().int().nonnegative().safe(),
  instances: z.array(projectRunInstanceSchema).max(100),
}).superRefine((state, context) => {
  if (state.isRunning !== state.instances.some((instance) => instance.isRunning)) {
    context.addIssue({ code: "custom", message: "项目运行状态与实例不一致。" });
  }
  if (state.isBusy !== state.instances.some((instance) => instance.isBusy)) {
    context.addIssue({ code: "custom", message: "项目忙碌状态与实例不一致。" });
  }
  if ((state.status === "running") !== state.isRunning) {
    context.addIssue({ code: "custom", message: "项目运行状态标签不一致。" });
  }
});

export const projectRunChangeSnapshotSchema = z.object({
  timedOut: z.boolean(),
  state: projectRunStateSchema,
});

export const projectFavoriteSnapshotSchema = z.object({
  projectIds: z.array(identifierSchema).max(10_000),
});

export const projectRemovalReceiptSchema = z.object({
  projectId: identifierSchema,
  removedRevision: z.number().int().nonnegative().safe(),
  registryRevision: z.number().int().positive().safe(),
  directoryPreserved: z.literal(true),
  preferencesCleaned: z.boolean(),
});

export type ProjectRunTarget = z.infer<typeof projectRunTargetSchema>;
export type ProjectRunCatalog = z.infer<typeof projectRunCatalogSchema>;
export type ProjectRunToolchainOption = z.infer<typeof projectRunToolchainOptionSchema>;
export type ProjectRunConfigurationFile = z.infer<typeof projectRunConfigurationFileSchema>;
export type ProjectRunValidationIssue = z.infer<typeof projectRunValidationIssueSchema>;
export type ProjectRunEnvironment = z.infer<typeof projectRunEnvironmentSchema>;
export type ProjectRunAnalysisSnapshot = z.infer<typeof projectRunAnalysisSnapshotSchema>;
export type ProjectRunInstance = z.infer<typeof projectRunInstanceSchema>;
export type ProjectRunState = z.infer<typeof projectRunStateSchema>;
export type ProjectRunChangeSnapshot = z.infer<typeof projectRunChangeSnapshotSchema>;
export type ProjectFavoriteSnapshot = z.infer<typeof projectFavoriteSnapshotSchema>;
export type ProjectRemovalReceipt = z.infer<typeof projectRemovalReceiptSchema>;

export interface ProjectRunEnvironmentDraft {
  id: string;
  key: string;
  value: string;
  hasStoredValue: boolean;
  valueDirty: boolean;
}

export function environmentDrafts(environment: ProjectRunEnvironment): ProjectRunEnvironmentDraft[] {
  return environment.environmentVariables.map((variable, index) => ({
    id: `stored-${index}-${variable.key}`,
    key: variable.key,
    value: "",
    hasStoredValue: variable.hasValue,
    valueDirty: false,
  }));
}

export function selectedProjectRunTarget(catalog: ProjectRunCatalog | null) {
  if (!catalog) return null;
  return catalog.targets.find((target) => target.id === catalog.defaultTargetId)
    ?? catalog.targets[0]
    ?? null;
}

export function requiredProjectRunToolchains(catalog: ProjectRunCatalog | null) {
  return [...new Set(selectedProjectRunTarget(catalog)?.requiredToolchains ?? [])].sort();
}
