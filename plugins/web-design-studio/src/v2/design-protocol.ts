import type { SceneLayoutMode, SceneVariableType } from './scene-schema.js';

export interface DesignScope {
  projectId: string;
  documentId: string;
}

export interface DesignBriefSection {
  sectionKey: string;
  role: string;
  goal: string;
  contentRequirements: string[];
}

export interface DesignBriefPage {
  pageKey: string;
  name: string;
  purpose: string;
  sections: DesignBriefSection[];
}

export interface DesignBrief {
  schemaVersion: 1;
  briefId: string;
  scope: DesignScope;
  objective: string;
  audience: string[];
  brand: {
    name: string;
    attributes: string[];
    visualReferences: string[];
  };
  content: {
    locale: string;
    pages: DesignBriefPage[];
  };
  constraints: {
    viewportWidths: number[];
    accessibilityLevel: 'AA' | 'AAA';
    forbiddenPatterns: string[];
  };
  createdAt: string;
}

export interface DesignTokenIntent {
  tokenKey: string;
  type: SceneVariableType;
  purpose: string;
  modes: string[];
}

export type DesignLayoutIntent =
  | {
      mode: Extract<SceneLayoutMode, 'auto'>;
      direction: 'horizontal' | 'vertical';
      wrap: boolean;
      widthBehavior: 'full-bleed' | 'bounded' | 'content';
      maxContentWidth?: number;
    }
  | {
      mode: Extract<SceneLayoutMode, 'grid'>;
      minColumnWidth: number;
      widthBehavior: 'full-bleed' | 'bounded';
      maxContentWidth?: number;
    }
  | {
      mode: Extract<SceneLayoutMode, 'free'>;
      composition: 'editorial' | 'mosaic' | 'layered';
      widthBehavior: 'full-bleed' | 'bounded';
      maxContentWidth?: number;
    };

export interface DesignSpecSection {
  sectionKey: string;
  nodeRole: string;
  visualPriority: 'primary' | 'secondary' | 'supporting';
  layoutIntent: DesignLayoutIntent;
  componentStrategy: {
    source: 'native' | 'library' | 'mixed';
    preferredLibraries: string[];
    requiredInteractions: string[];
  };
  responsiveIntent: string[];
}

export interface DesignSpecPage {
  pageKey: string;
  sections: DesignSpecSection[];
}

export interface DesignSpec {
  schemaVersion: 1;
  specId: string;
  briefId: string;
  scope: DesignScope;
  sourceRevision: number;
  designPrinciples: string[];
  tokenIntents: DesignTokenIntent[];
  pages: DesignSpecPage[];
  globalResponsiveIntent: string[];
  createdAt: string;
}

export type DesignGenerationStep =
  | { stepId: string; kind: 'create-design-system'; tokenKeys: string[]; dependsOn: string[] }
  | { stepId: string; kind: 'apply-section-transaction'; pageKey: string; sectionKey: string; nodeRole: string; dependsOn: string[] }
  | { stepId: string; kind: 'solve-layout'; pageKey: string; viewportWidths: number[]; dependsOn: string[] }
  | { stepId: string; kind: 'render-snapshots'; pageKey: string; viewportWidths: number[]; dependsOn: string[] }
  | { stepId: string; kind: 'critique-and-revise'; pageKey: string; dependsOn: string[] };

export interface DesignGenerationPlan {
  planId: string;
  briefId: string;
  specId: string;
  scope: DesignScope;
  baseRevision: number;
  steps: DesignGenerationStep[];
}

const identifierPattern = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/;

function assertIdentifier(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !identifierPattern.test(value)) throw new Error(`${label} is invalid.`);
}

function assertText(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !value.trim()) throw new Error(`${label} is required.`);
}

function assertTextList(value: unknown, label: string, minimum = 1): asserts value is string[] {
  if (!Array.isArray(value) || value.length < minimum || value.some((item) => typeof item !== 'string' || !item.trim())) {
    throw new Error(`${label} needs at least ${minimum} non-empty values.`);
  }
}

function assertUnique(values: string[], label: string): void {
  if (new Set(values).size !== values.length) throw new Error(`${label} must be unique.`);
}

function assertScope(scope: DesignScope, label: string): void {
  if (!scope || typeof scope !== 'object') throw new Error(`${label} is invalid.`);
  assertIdentifier(scope.projectId, `${label}.projectId`);
  assertIdentifier(scope.documentId, `${label}.documentId`);
}

function assertTimestamp(value: string, label: string): void {
  if (!Number.isFinite(Date.parse(value))) throw new Error(`${label} is invalid.`);
}

export function assertDesignBrief(value: unknown): asserts value is DesignBrief {
  if (!value || typeof value !== 'object') throw new Error('Design brief must be an object.');
  const brief = value as DesignBrief;
  if (brief.schemaVersion !== 1) throw new Error('Design brief schemaVersion must be 1.');
  assertIdentifier(brief.briefId, 'briefId');
  assertScope(brief.scope, 'brief.scope');
  assertText(brief.objective, 'brief.objective');
  assertTextList(brief.audience, 'brief.audience');
  if (!brief.brand || typeof brief.brand !== 'object') throw new Error('brief.brand is invalid.');
  assertText(brief.brand.name, 'brief.brand.name');
  assertTextList(brief.brand.attributes, 'brief.brand.attributes', 2);
  assertTextList(brief.brand.visualReferences, 'brief.brand.visualReferences', 0);
  if (!brief.content || typeof brief.content !== 'object') throw new Error('brief.content is invalid.');
  assertText(brief.content.locale, 'brief.content.locale');
  if (!Array.isArray(brief.content.pages) || brief.content.pages.length === 0) throw new Error('brief.content.pages needs at least one page.');
  assertUnique(brief.content.pages.map((page) => page.pageKey), 'brief page keys');
  for (const page of brief.content.pages) {
    assertIdentifier(page.pageKey, 'brief.page.pageKey');
    assertText(page.name, `brief.page.${page.pageKey}.name`);
    assertText(page.purpose, `brief.page.${page.pageKey}.purpose`);
    if (!Array.isArray(page.sections) || page.sections.length === 0) throw new Error(`brief.page.${page.pageKey}.sections needs at least one section.`);
    assertUnique(page.sections.map((section) => section.sectionKey), `brief.page.${page.pageKey} section keys`);
    for (const section of page.sections) {
      assertIdentifier(section.sectionKey, 'brief.section.sectionKey');
      assertText(section.role, `brief.section.${section.sectionKey}.role`);
      assertText(section.goal, `brief.section.${section.sectionKey}.goal`);
      assertTextList(section.contentRequirements, `brief.section.${section.sectionKey}.contentRequirements`);
    }
  }
  if (!brief.constraints || typeof brief.constraints !== 'object') throw new Error('brief.constraints is invalid.');
  if (!Array.isArray(brief.constraints.viewportWidths) || brief.constraints.viewportWidths.length < 2
    || brief.constraints.viewportWidths.some((width) => !Number.isSafeInteger(width) || width < 240 || width > 10000)) {
    throw new Error('brief.constraints.viewportWidths needs at least two valid CSS widths.');
  }
  assertUnique(brief.constraints.viewportWidths.map(String), 'brief viewport widths');
  if (brief.constraints.accessibilityLevel !== 'AA' && brief.constraints.accessibilityLevel !== 'AAA') throw new Error('brief.constraints.accessibilityLevel is invalid.');
  assertTextList(brief.constraints.forbiddenPatterns, 'brief.constraints.forbiddenPatterns');
  assertTimestamp(brief.createdAt, 'brief.createdAt');
}

function assertLayoutIntent(intent: DesignLayoutIntent, label: string): void {
  if (!intent || typeof intent !== 'object') throw new Error(`${label} is invalid.`);
  if (intent.mode === 'auto') {
    if (intent.direction !== 'horizontal' && intent.direction !== 'vertical') throw new Error(`${label}.direction is invalid.`);
    if (typeof intent.wrap !== 'boolean') throw new Error(`${label}.wrap is invalid.`);
    if (!['full-bleed', 'bounded', 'content'].includes(intent.widthBehavior)) throw new Error(`${label}.widthBehavior is invalid.`);
  } else if (intent.mode === 'grid') {
    if (!Number.isFinite(intent.minColumnWidth) || intent.minColumnWidth < 80) throw new Error(`${label}.minColumnWidth is invalid.`);
    if (intent.widthBehavior !== 'full-bleed' && intent.widthBehavior !== 'bounded') throw new Error(`${label}.widthBehavior is invalid.`);
  } else if (intent.mode === 'free') {
    if (!['editorial', 'mosaic', 'layered'].includes(intent.composition)) throw new Error(`${label}.composition is invalid.`);
    if (intent.widthBehavior !== 'full-bleed' && intent.widthBehavior !== 'bounded') throw new Error(`${label}.widthBehavior is invalid.`);
  } else {
    throw new Error(`${label}.mode is invalid.`);
  }
  if (intent.maxContentWidth !== undefined && (!Number.isFinite(intent.maxContentWidth) || intent.maxContentWidth < 240 || intent.maxContentWidth > 4000)) {
    throw new Error(`${label}.maxContentWidth is invalid.`);
  }
  if (intent.widthBehavior === 'bounded' && intent.maxContentWidth === undefined) throw new Error(`${label}.maxContentWidth is required for bounded content.`);
}

export function assertDesignSpec(value: unknown): asserts value is DesignSpec {
  if (!value || typeof value !== 'object') throw new Error('Design spec must be an object.');
  const spec = value as DesignSpec;
  if (spec.schemaVersion !== 1) throw new Error('Design spec schemaVersion must be 1.');
  assertIdentifier(spec.specId, 'specId');
  assertIdentifier(spec.briefId, 'spec.briefId');
  assertScope(spec.scope, 'spec.scope');
  if (!Number.isSafeInteger(spec.sourceRevision) || spec.sourceRevision < 0) throw new Error('spec.sourceRevision is invalid.');
  assertTextList(spec.designPrinciples, 'spec.designPrinciples', 2);
  if (!Array.isArray(spec.tokenIntents) || spec.tokenIntents.length === 0) throw new Error('spec.tokenIntents needs at least one token.');
  assertUnique(spec.tokenIntents.map((token) => token.tokenKey), 'spec token keys');
  for (const token of spec.tokenIntents) {
    assertIdentifier(token.tokenKey, 'spec.token.tokenKey');
    if (!['color', 'number', 'string', 'boolean', 'duration', 'easing'].includes(token.type)) throw new Error(`spec.token.${token.tokenKey}.type is invalid.`);
    assertText(token.purpose, `spec.token.${token.tokenKey}.purpose`);
    assertTextList(token.modes, `spec.token.${token.tokenKey}.modes`);
    assertUnique(token.modes, `spec.token.${token.tokenKey}.modes`);
  }
  if (!Array.isArray(spec.pages) || spec.pages.length === 0) throw new Error('spec.pages needs at least one page.');
  assertUnique(spec.pages.map((page) => page.pageKey), 'spec page keys');
  for (const page of spec.pages) {
    assertIdentifier(page.pageKey, 'spec.page.pageKey');
    if (!Array.isArray(page.sections) || page.sections.length === 0) throw new Error(`spec.page.${page.pageKey}.sections needs at least one section.`);
    assertUnique(page.sections.map((section) => section.sectionKey), `spec.page.${page.pageKey} section keys`);
    for (const section of page.sections) {
      assertIdentifier(section.sectionKey, 'spec.section.sectionKey');
      assertText(section.nodeRole, `spec.section.${section.sectionKey}.nodeRole`);
      if (!['primary', 'secondary', 'supporting'].includes(section.visualPriority)) throw new Error(`spec.section.${section.sectionKey}.visualPriority is invalid.`);
      assertLayoutIntent(section.layoutIntent, `spec.section.${section.sectionKey}.layoutIntent`);
      if (!section.componentStrategy || typeof section.componentStrategy !== 'object'
        || !['native', 'library', 'mixed'].includes(section.componentStrategy.source)) throw new Error(`spec.section.${section.sectionKey}.componentStrategy is invalid.`);
      assertTextList(section.componentStrategy.preferredLibraries, `spec.section.${section.sectionKey}.preferredLibraries`, 0);
      assertTextList(section.componentStrategy.requiredInteractions, `spec.section.${section.sectionKey}.requiredInteractions`, 0);
      if (section.componentStrategy.source === 'library' && section.componentStrategy.preferredLibraries.length === 0) {
        throw new Error(`spec.section.${section.sectionKey} needs a preferred library.`);
      }
      assertTextList(section.responsiveIntent, `spec.section.${section.sectionKey}.responsiveIntent`);
    }
  }
  assertTextList(spec.globalResponsiveIntent, 'spec.globalResponsiveIntent');
  assertTimestamp(spec.createdAt, 'spec.createdAt');
}

export function assertDesignSpecMatchesBrief(spec: DesignSpec, brief: DesignBrief): void {
  assertDesignBrief(brief);
  assertDesignSpec(spec);
  if (spec.briefId !== brief.briefId) throw new Error('Design spec references a different brief.');
  if (spec.scope.projectId !== brief.scope.projectId || spec.scope.documentId !== brief.scope.documentId) {
    throw new Error('Design spec scope must exactly preserve the brief projectId and documentId.');
  }
  const specPages = new Map(spec.pages.map((page) => [page.pageKey, page]));
  for (const briefPage of brief.content.pages) {
    const specPage = specPages.get(briefPage.pageKey);
    if (!specPage) throw new Error(`Design spec is missing page ${briefPage.pageKey}.`);
    const sectionKeys = new Set(specPage.sections.map((section) => section.sectionKey));
    for (const section of briefPage.sections) if (!sectionKeys.has(section.sectionKey)) throw new Error(`Design spec is missing section ${briefPage.pageKey}/${section.sectionKey}.`);
  }
  for (const specPage of spec.pages) if (!brief.content.pages.some((page) => page.pageKey === specPage.pageKey)) throw new Error(`Design spec adds undeclared page ${specPage.pageKey}.`);
}

export function createDesignGenerationPlan(brief: DesignBrief, spec: DesignSpec): DesignGenerationPlan {
  assertDesignSpecMatchesBrief(spec, brief);
  const designSystemStepId = `${spec.specId}:design-system`;
  const steps: DesignGenerationStep[] = [{
    stepId: designSystemStepId,
    kind: 'create-design-system',
    tokenKeys: spec.tokenIntents.map((token) => token.tokenKey),
    dependsOn: []
  }];
  for (const page of spec.pages) {
    const sectionStepIds: string[] = [];
    for (const section of page.sections) {
      const stepId = `${spec.specId}:${page.pageKey}:${section.sectionKey}`;
      steps.push({
        stepId,
        kind: 'apply-section-transaction',
        pageKey: page.pageKey,
        sectionKey: section.sectionKey,
        nodeRole: section.nodeRole,
        dependsOn: [designSystemStepId, ...sectionStepIds.slice(-1)]
      });
      sectionStepIds.push(stepId);
    }
    const solveStepId = `${spec.specId}:${page.pageKey}:solve`;
    const snapshotStepId = `${spec.specId}:${page.pageKey}:snapshots`;
    steps.push({ stepId: solveStepId, kind: 'solve-layout', pageKey: page.pageKey, viewportWidths: [...brief.constraints.viewportWidths], dependsOn: [...sectionStepIds] });
    steps.push({ stepId: snapshotStepId, kind: 'render-snapshots', pageKey: page.pageKey, viewportWidths: [...brief.constraints.viewportWidths], dependsOn: [solveStepId] });
    steps.push({ stepId: `${spec.specId}:${page.pageKey}:critique`, kind: 'critique-and-revise', pageKey: page.pageKey, dependsOn: [snapshotStepId] });
  }
  return {
    planId: `plan:${spec.specId}`,
    briefId: brief.briefId,
    specId: spec.specId,
    scope: structuredClone(spec.scope),
    baseRevision: spec.sourceRevision,
    steps
  };
}
