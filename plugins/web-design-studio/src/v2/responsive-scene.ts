import {
  assertSceneDocument,
  indexSceneDocument,
  isSceneContainer,
  mergeSceneLayout,
  type SceneDocument,
  type SceneNode,
  type SceneResponsiveRule
} from './scene-schema.js';
import { SceneVariableResolver, type SceneVariableModeSelection } from './scene-variables.js';

export interface ResolvedResponsiveScene {
  document: SceneDocument;
  activeRuleIds: string[];
  variableModes: SceneVariableModeSelection;
}

function ruleMatches(rule: SceneResponsiveRule, viewportWidth: number): boolean {
  return (rule.minWidth === undefined || viewportWidth >= rule.minWidth)
    && (rule.maxWidth === undefined || viewportWidth < rule.maxWidth);
}

function setAtPath(target: unknown, path: string[], value: unknown): void {
  let cursor = target as Record<string, unknown>;
  for (const segment of path.slice(0, -1)) cursor = cursor[segment] as Record<string, unknown>;
  cursor[path.at(-1)!] = structuredClone(value);
}

export function resolveResponsiveScene(document: SceneDocument, viewportWidth: number): ResolvedResponsiveScene {
  assertSceneDocument(document);
  if (!Number.isFinite(viewportWidth) || viewportWidth <= 0) throw new Error('Responsive viewport width must be greater than zero.');
  const result = structuredClone(document);
  const index = indexSceneDocument(result);
  const activeRules = result.responsiveRules.filter((rule) => ruleMatches(rule, viewportWidth));
  const variableModes: SceneVariableModeSelection = Object.fromEntries(result.variableCollections.map((collection) => [collection.id, collection.modes[0].id]));
  for (const rule of activeRules) {
    Object.assign(variableModes, rule.variableModes);
    for (const override of rule.nodeOverrides) {
      const node = index.get(override.nodeId)!.node;
      if (override.visible !== undefined) node.visible = override.visible;
      if (override.layout) node.layout = mergeSceneLayout(node.layout, override.layout);
      if (override.childOrder) {
        if (!isSceneContainer(node)) throw new Error(`Responsive child order target ${node.id} is not a container.`);
        const children = new Map(node.children.map((child) => [child.id, child]));
        node.children = override.childOrder.map((id) => children.get(id)!);
      }
    }
  }
  const resolver = new SceneVariableResolver(result);
  for (const entry of indexSceneDocument(result).values()) {
    for (const [propertyPath, resolved] of Object.entries(resolver.resolveNode(entry.node.id, variableModes))) {
      setAtPath(entry.node, propertyPath.split('.'), resolved.value);
    }
  }
  assertSceneDocument(result);
  return { document: result, activeRuleIds: activeRules.map((rule) => rule.id), variableModes };
}
