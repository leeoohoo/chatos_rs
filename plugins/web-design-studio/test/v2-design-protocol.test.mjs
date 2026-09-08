import assert from 'node:assert/strict';
import test from 'node:test';
import {
  assertDesignBrief,
  assertDesignSpec,
  assertDesignSpecMatchesBrief,
  createDesignGenerationPlan
} from '../dist/v2-design-protocol.test.mjs';

function brief() {
  return {
    schemaVersion: 1,
    briefId: 'brief-saas',
    scope: { projectId: 'project-42', documentId: 'design-42' },
    objective: 'Explain an AI design product and convert qualified teams.',
    audience: ['Product leaders', 'Design teams'],
    brand: { name: 'Arc', attributes: ['precise', 'calm'], visualReferences: ['editorial product photography'] },
    content: {
      locale: 'zh-CN',
      pages: [{
        pageKey: 'home', name: '首页', purpose: 'Product introduction',
        sections: [
          { sectionKey: 'hero', role: 'hero', goal: 'State the value', contentRequirements: ['headline', 'product visual'] },
          { sectionKey: 'proof', role: 'social-proof', goal: 'Build trust', contentRequirements: ['customer evidence'] }
        ]
      }]
    },
    constraints: { viewportWidths: [390, 1440, 3840], accessibilityLevel: 'AA', forbiddenPatterns: ['dashboard as landing page'] },
    createdAt: '2026-09-08T00:00:00.000Z'
  };
}

function spec() {
  return {
    schemaVersion: 1,
    specId: 'spec-saas',
    briefId: 'brief-saas',
    scope: { projectId: 'project-42', documentId: 'design-42' },
    sourceRevision: 7,
    designPrinciples: ['Product UI is the primary evidence', 'Use restrained editorial rhythm'],
    tokenIntents: [
      { tokenKey: 'color-surface', type: 'color', purpose: 'Page surface', modes: ['light', 'dark'] },
      { tokenKey: 'space-section', type: 'number', purpose: 'Section rhythm', modes: ['comfortable'] }
    ],
    pages: [{
      pageKey: 'home',
      sections: [
        {
          sectionKey: 'hero', nodeRole: 'hero', visualPriority: 'primary',
          layoutIntent: { mode: 'auto', direction: 'horizontal', wrap: false, widthBehavior: 'bounded', maxContentWidth: 1440 },
          componentStrategy: { source: 'mixed', preferredLibraries: ['Ant Design'], requiredInteractions: ['primary CTA'] },
          responsiveIntent: ['Stack copy above product visual below tablet']
        },
        {
          sectionKey: 'proof', nodeRole: 'social-proof', visualPriority: 'secondary',
          layoutIntent: { mode: 'grid', minColumnWidth: 240, widthBehavior: 'bounded', maxContentWidth: 1200 },
          componentStrategy: { source: 'native', preferredLibraries: [], requiredInteractions: [] },
          responsiveIntent: ['Reduce columns continuously without device copies']
        }
      ]
    }],
    globalResponsiveIntent: ['Full-bleed surface with centered bounded content'],
    createdAt: '2026-09-08T00:01:00.000Z'
  };
}

test('brief and design spec preserve the transmitted project and document scope', () => {
  const inputBrief = brief();
  const inputSpec = spec();
  assert.doesNotThrow(() => assertDesignBrief(inputBrief));
  assert.doesNotThrow(() => assertDesignSpec(inputSpec));
  assert.doesNotThrow(() => assertDesignSpecMatchesBrief(inputSpec, inputBrief));
  inputSpec.scope.projectId = 'project-other';
  assert.throws(() => assertDesignSpecMatchesBrief(inputSpec, inputBrief), /exactly preserve/);
});

test('generation plans create one semantic section per transaction before visual critique', () => {
  const plan = createDesignGenerationPlan(brief(), spec());
  assert.deepEqual(plan.scope, { projectId: 'project-42', documentId: 'design-42' });
  assert.equal(plan.baseRevision, 7);
  assert.deepEqual(plan.steps.map((step) => step.kind), [
    'create-design-system',
    'apply-section-transaction',
    'apply-section-transaction',
    'solve-layout',
    'render-snapshots',
    'critique-and-revise'
  ]);
  const sections = plan.steps.filter((step) => step.kind === 'apply-section-transaction');
  assert.deepEqual(sections.map((step) => step.sectionKey), ['hero', 'proof']);
  assert.deepEqual(plan.steps.at(-1).dependsOn, ['spec-saas:home:snapshots']);
});

test('protocol rejects duplicate sections, coordinate-first specs, and undeclared content', () => {
  const duplicate = brief();
  duplicate.content.pages[0].sections.push(structuredClone(duplicate.content.pages[0].sections[0]));
  assert.throws(() => assertDesignBrief(duplicate), /section keys must be unique/);
  const invalidSpec = spec();
  invalidSpec.pages[0].sections[0].layoutIntent = { mode: 'absolute', x: 10, y: 20 };
  assert.throws(() => assertDesignSpec(invalidSpec), /layoutIntent.mode is invalid/);
  const undeclared = spec();
  undeclared.pages.push({ ...structuredClone(undeclared.pages[0]), pageKey: 'pricing' });
  assert.throws(() => assertDesignSpecMatchesBrief(undeclared, brief()), /undeclared page pricing/);
});
