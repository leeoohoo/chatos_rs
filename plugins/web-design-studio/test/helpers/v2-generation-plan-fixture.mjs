import { createGenerationPlan } from '../../dist/v2-generation-plan-schema.test.mjs';

export function generationPlanFixture() {
  const design = (direction) => ({
    artDirection: `${direction} editorial product design`,
    compositionIntent: 'Strong visual focus followed by alternating editorial sections',
    typographyIntent: 'Large display type with restrained readable body copy',
    imageStrategy: 'Purposeful full-bleed product imagery with consistent art direction',
    contentHierarchy: ['Primary promise', 'Evidence', 'Detailed explanation', 'Call to action'],
    designAcceptanceCriteria: ['The page has one clear visual focus', 'The composition does not resemble a generic admin dashboard'],
    interactionIntents: ['Show essential navigation and button states only after visual approval']
  });
  const steps = (prefix) => [
    { stepId: `${prefix}-structure`, title: '建立页面骨架', kind: 'structure', target: { viewportWidths: [390, 1440] } },
    { stepId: `${prefix}-visual`, title: '完成主要视觉设计', kind: 'visual', dependsOn: [`${prefix}-structure`], target: { viewportWidths: [390, 1440] } },
    { stepId: `${prefix}-design-gate`, title: '视觉设计验收', kind: 'design-gate', dependsOn: [`${prefix}-visual`], target: { viewportWidths: [390, 1440] } },
    { stepId: `${prefix}-interaction`, title: '补充必要交互状态', kind: 'interaction', required: false, dependsOn: [`${prefix}-design-gate`], target: { viewportWidths: [390, 1440] } },
    { stepId: `${prefix}-handoff`, title: '页面最终验收', kind: 'handoff', dependsOn: [`${prefix}-design-gate`, `${prefix}-interaction`], target: { viewportWidths: [390, 1440] } }
  ];
  return createGenerationPlan({
    planId: 'plan-ai-site',
    scope: { projectId: 'project-host-scope', documentId: 'website-ai-site' },
    objective: 'Design a visually distinctive AI product website',
    audience: ['Product leaders', 'Design teams'],
    pages: [
      { pageId: 'home', name: '首页', purpose: '建立品牌与产品价值', design: design('Warm'), steps: steps('home') },
      { pageId: 'pricing', name: '价格', purpose: '清晰解释方案和购买决策', design: design('Precise'), steps: steps('pricing') }
    ]
  }, '2026-09-08T08:00:00.000Z');
}
