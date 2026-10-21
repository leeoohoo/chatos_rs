import { z } from "zod";

export const agentSkillKindSchema = z.enum(["profession", "projectType"]);

export const agentSkillDefinitionSchema = z.object({
  kind: agentSkillKindSchema,
  key: z.string().trim().min(1).max(200),
  label: z.string().trim().min(1).max(120),
  labelEn: z.string().trim().min(1).max(120),
  description: z.string().trim().min(1).max(4_000),
  descriptionEn: z.string().trim().min(1).max(4_000),
  categoryKey: z.string().trim().min(1).max(200),
  categoryLabel: z.string().trim().min(1).max(200),
  categoryLabelEn: z.string().trim().min(1).max(200),
  markdown: z.string().trim().min(1).max(500_000),
  markdownEn: z.string().trim().min(1).max(500_000),
  customized: z.boolean(),
}).strict();

export const agentSkillCatalogSchema = z.array(agentSkillDefinitionSchema).length(60)
  .superRefine((items, context) => {
    const identities = new Set<string>();
    items.forEach((item, index) => {
      const identity = `${item.kind}:${item.key}`;
      if (identities.has(identity)) {
        context.addIssue({
          code: "custom",
          message: "Skill 标识不能重复。",
          path: [index, "key"],
        });
      }
      identities.add(identity);
    });
  });

export type AgentSkillKind = z.infer<typeof agentSkillKindSchema>;
export type AgentSkillDefinition = z.infer<typeof agentSkillDefinitionSchema>;

export interface AgentSkillMutationInput {
  kind: AgentSkillKind;
  key: string;
  label: string;
  labelEn: string;
  description: string;
  descriptionEn: string;
  markdown: string;
  markdownEn: string;
}
