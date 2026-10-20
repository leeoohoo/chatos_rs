import { z } from "zod";

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
  protocolVersion: z.number().int().positive(),
  detail: z.string().min(1),
  retryable: z.boolean(),
});

export type HostLifecycleSnapshot = z.infer<typeof hostLifecycleSnapshotSchema>;

export const hostPhaseLabels: Record<HostLifecycleSnapshot["phase"], string> = {
  stopped: "本地服务未启动",
  launching: "正在启动本地服务",
  handshaking: "正在验证本地服务",
  synchronizing: "正在同步工作区",
  ready: "本地服务已就绪",
  degraded: "本地服务需要恢复",
  restarting: "正在恢复本地服务",
};
