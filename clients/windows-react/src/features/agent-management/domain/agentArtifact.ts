import { z } from "zod";

const sha256Schema = z.string().regex(/^[a-f0-9]{64}$/u);

export const agentArtifactSchema = z.object({
  artifactId: z.string().trim().min(1).max(512),
  name: z.string().trim().min(1).max(1_000),
  mimeType: z.string().trim().min(1).max(255),
  size: z.number().int().positive().max(2 * 1_024 * 1_024),
  sha256: sha256Schema,
  status: z.literal("stored"),
  createdAtUnixMs: z.number().int(),
  updatedAtUnixMs: z.number().int(),
}).strict();

export const agentArtifactPageSchema = z.object({
  artifacts: z.array(agentArtifactSchema).max(100),
  nextCursor: z.string().min(1).max(300).nullable(),
}).strict();

export const agentArtifactPreviewSchema = z.object({
  artifactId: z.string().trim().min(1).max(512),
  markdown: z.string().max(2 * 1_024 * 1_024),
}).strict();

export type AgentArtifact = z.infer<typeof agentArtifactSchema>;
export type AgentArtifactPage = z.infer<typeof agentArtifactPageSchema>;
export type AgentArtifactPreview = z.infer<typeof agentArtifactPreviewSchema>;
