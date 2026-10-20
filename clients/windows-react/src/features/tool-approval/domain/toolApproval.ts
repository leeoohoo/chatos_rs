import { z } from "zod";

export const toolApprovalDecisionSchema = z.enum(["accept", "decline"]);

export const toolApprovalSchema = z.object({
  invocationId: z.string().trim().min(1),
  invocationVersion: z.number().int().positive(),
  runId: z.string().trim().min(1),
  toolName: z.string().trim().min(1),
  command: z.string().min(1),
  purpose: z.string().nullable(),
  cwd: z.string().min(1),
  source: z.string().min(1),
  risk: z.enum(["low", "medium", "high"]),
  reason: z.string().nullable(),
  createdAtUnixMs: z.number().int(),
  updatedAtUnixMs: z.number().int(),
  availableDecisions: z.array(toolApprovalDecisionSchema),
}).strict();

export const toolApprovalsSchema = z.array(toolApprovalSchema);

export const toolApprovalResolutionSchema = z.object({
  invocationId: z.string().trim().min(1),
  invocationVersion: z.number().int().positive(),
  decision: toolApprovalDecisionSchema,
}).strict();

export type ToolApproval = z.infer<typeof toolApprovalSchema>;
export type ToolApprovalDecision = z.infer<typeof toolApprovalDecisionSchema>;
export type ToolApprovalResolution = z.infer<typeof toolApprovalResolutionSchema>;

export function allowsToolApprovalDecision(
  approval: ToolApproval,
  decision: ToolApprovalDecision,
) {
  return approval.availableDecisions.includes(decision);
}

export function riskPresentation(risk: ToolApproval["risk"]) {
  return {
    high: { label: "高风险", symbol: "!" },
    medium: { label: "中风险", symbol: "!" },
    low: { label: "低风险", symbol: "✓" },
  }[risk];
}
