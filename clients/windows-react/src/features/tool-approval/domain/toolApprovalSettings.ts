import { z } from "zod";
import { toolApprovalDecisionSchema } from "./toolApproval";

export const toolApprovalModeSchema = z.enum([
  "request_approval",
  "auto_approval",
  "full_control",
]);

export const toolApprovalHistoryEntrySchema = z.object({
  id: z.string().trim().min(1),
  invocationId: z.string().trim().min(1),
  invocationVersion: z.number().int().positive(),
  command: z.string().min(1),
  purpose: z.string().nullable(),
  cwd: z.string().min(1),
  source: z.string().min(1),
  mode: toolApprovalModeSchema,
  decision: toolApprovalDecisionSchema,
  risk: z.enum(["low", "medium", "high"]),
  reason: z.string().nullable(),
  createdAtUnixMs: z.number().int().positive(),
}).strict();

export const toolApprovalModelOptionSchema = z.object({
  id: z.string().trim().min(1),
  displayName: z.string().trim().min(1),
  modelName: z.string().trim().min(1),
  provider: z.string().trim().min(1),
  supportsReasoning: z.boolean(),
  thinkingLevels: z.array(z.string().trim().min(1)),
  defaultThinkingLevel: z.string().trim().min(1),
}).strict();

export const toolApprovalSettingsSchema = z.object({
  defaultMode: toolApprovalModeSchema,
  selectedModelId: z.string().trim().min(1).nullable(),
  selectedThinkingLevel: z.string().trim().min(1).nullable(),
  models: z.array(toolApprovalModelOptionSchema),
  automaticReviewerReady: z.boolean(),
  automaticReviewerDetail: z.string().min(1),
  history: z.array(toolApprovalHistoryEntrySchema).max(1_000),
}).strict();

export type ToolApprovalMode = z.infer<typeof toolApprovalModeSchema>;
export type ToolApprovalHistoryEntry = z.infer<typeof toolApprovalHistoryEntrySchema>;
export type ToolApprovalModelOption = z.infer<typeof toolApprovalModelOptionSchema>;
export type ToolApprovalSettings = z.infer<typeof toolApprovalSettingsSchema>;

export const approvalModePresentation: Record<ToolApprovalMode, {
  label: string;
  description: string;
}> = {
  request_approval: {
    label: "需要确认",
    description: "敏感操作逐条确认；这是默认推荐策略。",
  },
  auto_approval: {
    label: "自动审批",
    description: "审批模型无法判断或 runtime 不可用时，仍会转交用户确认。",
  },
  full_control: {
    label: "完全控制",
    description: "任务可直接执行高风险操作，只适合完全受信任的环境。",
  },
};
