import { z } from "zod";
import type { MessageTaskGraph } from "../../../contracts/conversationRunContract";

export type RunInspectorGraph = MessageTaskGraph;
export type RunInspectorSection = "detail" | "process" | "run";

export const runInspectorProcessEntrySchema = z.object({
  id: z.string().min(1),
  heading: z.string().nullable(),
  content: z.string().min(1),
  createdAtUnixMs: z.number().int().nonnegative(),
});

export const runInspectorRunSchema = z.object({
  runId: z.string().min(1),
  taskId: z.string().min(1),
  status: z.string().min(1),
  iteration: z.number().int().nonnegative(),
  maxIterations: z.number().int().positive(),
  resultSummary: z.string().nullable(),
  reportContent: z.string().nullable(),
  errorMessage: z.string().nullable(),
  createdAtUnixMs: z.number().int().nonnegative(),
  updatedAtUnixMs: z.number().int().nonnegative(),
  finishedAtUnixMs: z.number().int().nonnegative().nullable(),
});

export const taskInspectorDetailSchema = z.object({
  task: z.object({
    taskId: z.string().min(1),
    title: z.string().min(1),
    status: z.string().min(1),
    version: z.number().int().positive(),
    description: z.string().nullable(),
    objective: z.string().nullable(),
    resultSummary: z.string().nullable(),
    processEntries: z.array(runInspectorProcessEntrySchema),
    lastRunId: z.string().min(1).nullable(),
    modelConfigRef: z.string().min(1),
    thinkingLevel: z.string().nullable(),
    createdAtUnixMs: z.number().int().nonnegative(),
    updatedAtUnixMs: z.number().int().nonnegative(),
  }),
  run: runInspectorRunSchema.nullable(),
});

export const runInspectorDetailSchema = z.object({
  run: runInspectorRunSchema,
  events: z.array(z.object({
    cursor: z.number().int().nonnegative(),
    eventId: z.string().min(1),
    eventType: z.string().min(1),
    summary: z.string().nullable(),
    status: z.string().nullable(),
    createdAtUnixMs: z.number().int().nonnegative(),
  })),
  eventsHasMore: z.boolean(),
  nextOffset: z.number().int().nonnegative(),
});

export const taskMutationReceiptSchema = z.object({
  taskId: z.string().min(1),
  status: z.string().min(1),
  activeRunId: z.string().min(1).nullable(),
  version: z.number().int().positive(),
});

export type TaskInspectorDetail = z.infer<typeof taskInspectorDetailSchema>;
export type RunInspectorDetail = z.infer<typeof runInspectorDetailSchema>;
export type TaskMutationReceipt = z.infer<typeof taskMutationReceiptSchema>;

export function isCancellableTaskStatus(status: string) {
  return [
    "pending", "ready", "queued", "running", "processing", "in_progress", "doing",
    "blocked", "reviewing_followup", "waiting_confirmation",
  ].includes(status.trim().toLowerCase());
}

export function isRetryableTaskStatus(status: string) {
  return ["blocked", "failed"].includes(status.trim().toLowerCase());
}

export function isRecoverableTaskStatus(status: string) {
  return ["failed", "cancelled", "canceled", "stopped"].includes(status.trim().toLowerCase());
}

export function graphHasActiveTasks(graph: RunInspectorGraph) {
  return graph.nodes.some(({ task }) => ["pending", "ready", "running"].includes(task.status));
}
