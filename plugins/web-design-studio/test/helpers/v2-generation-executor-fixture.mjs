import { createGenerationPlan } from '../../dist/v2-generation-plan-schema.test.mjs';

export function generationExecutorPlan() {
  return createGenerationPlan({
    planId: 'plan-candidate-executor',
    scope: { projectId: 'project-candidate', documentId: 'scene-test' },
    objective: 'Improve the visual hierarchy without turning the page into an interaction demo.',
    audience: ['Design reviewers'],
    pages: [{
      pageId: 'page-home',
      name: '首页',
      purpose: 'Present a visually strong product story',
      design: {
        artDirection: 'Editorial product design with strong typography',
        compositionIntent: 'One dominant visual focus with calm supporting regions',
        typographyIntent: 'Expressive display heading and restrained body copy',
        imageStrategy: 'Use imagery only when it strengthens the product story',
        contentHierarchy: ['Promise', 'Evidence', 'Action'],
        designAcceptanceCriteria: ['The primary heading is unmistakable', 'The page does not resemble an admin dashboard'],
        interactionIntents: []
      },
      steps: [
        {
          stepId: 'home-visual', title: '调整首页视觉层级', kind: 'visual',
          target: { nodeIds: ['text-hero-heading'], viewportWidths: [390, 1440] }
        },
        {
          stepId: 'home-design-gate', title: '首页视觉验收', kind: 'design-gate', dependsOn: ['home-visual'],
          target: { nodeIds: ['frame-desktop'], viewportWidths: [390, 1440] }
        },
        {
          stepId: 'home-interaction', title: '必要交互状态', kind: 'interaction', required: false, dependsOn: ['home-design-gate'],
          target: { nodeIds: ['frame-desktop'], viewportWidths: [390, 1440] }
        },
        {
          stepId: 'home-handoff', title: '首页最终验收', kind: 'handoff', dependsOn: ['home-design-gate', 'home-interaction'],
          target: { nodeIds: ['frame-desktop'], viewportWidths: [390, 1440] }
        }
      ]
    }]
  }, '2026-09-08T11:00:00.000Z');
}
