import { z } from "zod";
import type { MessageTaskGraph } from "../../../contracts/conversationRunContract";

export type RunInspectorGraph = MessageTaskGraph;

export const runInspectorDetailSchema = z.object({
  run: z.object({
    runId: z.string().min(1),
    taskId: z.string().min(1),
    status: z.string().min(1),
    iteration: z.number().int().nonnegative(),
    maxIterations: z.number().int().positive(),
    updatedAtUnixMs: z.number().int().nonnegative(),
  }),
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

export type RunInspectorDetail = z.infer<typeof runInspectorDetailSchema>;
