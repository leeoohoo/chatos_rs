import { z } from "zod";

const positiveIntegerSchema = z.number().int().positive();
const nonnegativeIntegerSchema = z.number().int().nonnegative();
const resourcePressureLevelSchema = z.enum(["normal", "elevated", "serious", "critical"]);

export const hostPhaseSchema = z.enum([
  "stopped",
  "launching",
  "handshaking",
  "synchronizing",
  "ready",
  "degraded",
  "restarting",
]);

export const hostLifecycleSnapshotSchema = z.object({
  phase: hostPhaseSchema,
  ready: z.boolean(),
  protocolVersion: positiveIntegerSchema,
  detail: z.string().min(1),
  retryable: z.boolean(),
});

export type HostLifecycleSnapshot = z.infer<typeof hostLifecycleSnapshotSchema>;

export const hostDiagnosticEntrySchema = z.object({
  sequence: positiveIntegerSchema,
  timestampUnixMs: nonnegativeIntegerSchema,
  event: z.string().min(1),
  outcome: z.string().min(1),
});

export const hostDiagnosticsSnapshotSchema = z.object({
  capacity: positiveIntegerSchema,
  entries: z.array(hostDiagnosticEntrySchema),
});

export type HostDiagnosticEntry = z.infer<typeof hostDiagnosticEntrySchema>;
export type HostDiagnosticsSnapshot = z.infer<typeof hostDiagnosticsSnapshotSchema>;

export const machineResourceDomainSnapshotSchema = z.object({
  domain: z.enum(["mediaWorker", "codeReadWorker"]),
  configuredLimit: nonnegativeIntegerSchema,
  activeCount: nonnegativeIntegerSchema,
  queuedCount: nonnegativeIntegerSchema,
});

export const machineResourceSnapshotSchema = z.object({
  poolId: z.string().min(1),
  configuredResourceUnits: nonnegativeIntegerSchema,
  baselineResourceUnits: nonnegativeIntegerSchema,
  targetResourceUnits: nonnegativeIntegerSchema,
  availableResourceUnits: nonnegativeIntegerSchema,
  maximumBackgroundResourceUnits: nonnegativeIntegerSchema,
  usedResourceUnits: nonnegativeIntegerSchema,
  queuedCount: nonnegativeIntegerSchema,
  pressureLevel: resourcePressureLevelSchema,
  totalLogicalJobs: z.number().int().min(2),
  baseHostWorkItems: positiveIntegerSchema,
  dynamicBorrowingEnabled: z.boolean(),
  domains: z.array(machineResourceDomainSnapshotSchema),
  host: z.object({
    maximumConcurrentWorkItems: positiveIntegerSchema,
    targetConcurrentWorkItems: positiveIntegerSchema,
    effectiveMaximumConcurrentWorkItems: positiveIntegerSchema,
    pressureLevel: resourcePressureLevelSchema,
    activeWorkItems: nonnegativeIntegerSchema,
    idleWorkers: nonnegativeIntegerSchema,
    queuedReadyWorkItems: nonnegativeIntegerSchema,
    acceptingNewWork: z.boolean(),
    activeMemorySyncItems: nonnegativeIntegerSchema,
    admissionInFlightWorkItems: nonnegativeIntegerSchema,
  }).nullable(),
});

export type MachineResourceSnapshot = z.infer<typeof machineResourceSnapshotSchema>;

export const hostPhaseLabels: Record<HostLifecycleSnapshot["phase"], string> = {
  stopped: "本地服务未启动",
  launching: "正在启动本地服务",
  handshaking: "正在验证本地服务",
  synchronizing: "正在同步工作区",
  ready: "本地服务已就绪",
  degraded: "本地服务需要恢复",
  restarting: "正在恢复本地服务",
};
