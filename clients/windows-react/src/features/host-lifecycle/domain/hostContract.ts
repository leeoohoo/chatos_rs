import { z } from "zod";

export const LOCAL_AGENT_PROTOCOL_VERSION = 49 as const;
export const LOCAL_AGENT_MAX_FRAME_BYTES = 4 * 1024 * 1024;
export const LOCAL_AGENT_MAX_MESSAGE_BYTES = 64 * 1024 * 1024;

const healthResultSchema = z.object({
  type: z.literal("health"),
  service: z.literal("chatos-local-agent-host"),
  storage_ready: z.literal(true),
  recovered_claims: z.number().int().nonnegative(),
  max_frame_bytes: z.literal(LOCAL_AGENT_MAX_FRAME_BYTES),
  max_message_bytes: z.literal(LOCAL_AGENT_MAX_MESSAGE_BYTES),
  scheduler: z.object({
    maximum_concurrent_work_items: z.number().int().positive(),
    effective_maximum_concurrent_work_items: z.number().int().positive(),
    pressure_level: z.enum(["normal", "elevated", "serious", "critical"]),
    active_work_items: z.number().int().nonnegative(),
    idle_workers: z.number().int().nonnegative(),
    queued_ready_work_items: z.number().int().nonnegative(),
  }),
});

const hostErrorSchema = z.object({
  code: z.string().min(1),
  message: z.string().min(1),
  retryable: z.boolean(),
});

export const healthRequestSchema = z.object({
  protocol_version: z.literal(LOCAL_AGENT_PROTOCOL_VERSION),
  command_id: z.string().min(1),
  command: z.object({ type: z.literal("health") }),
});

export const healthResponseSchema = z.discriminatedUnion("ok", [
  z.object({
    protocol_version: z.literal(LOCAL_AGENT_PROTOCOL_VERSION),
    command_id: z.string().min(1),
    ok: z.literal(true),
    result: healthResultSchema,
    error: z.undefined().optional(),
  }),
  z.object({
    protocol_version: z.literal(LOCAL_AGENT_PROTOCOL_VERSION),
    command_id: z.string().min(1),
    ok: z.literal(false),
    result: z.undefined().optional(),
    error: hostErrorSchema,
  }),
]);

export type HealthRequest = z.infer<typeof healthRequestSchema>;
export type HealthResponse = z.infer<typeof healthResponseSchema>;
