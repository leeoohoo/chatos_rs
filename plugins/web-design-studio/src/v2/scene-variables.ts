import {
  assertSceneDocument,
  indexSceneDocument,
  type SceneDocument,
  type SceneVariable,
  type SceneVariableCollection,
  type SceneVariableType,
  type SceneVariableValue
} from './scene-schema.js';

export type SceneVariableModeSelection = Record<string, string>;

export interface ResolvedSceneVariable {
  variableId: string;
  collectionId: string;
  modeId: string;
  type: SceneVariableType;
  value: SceneVariableValue;
  chain: Array<{ variableId: string; collectionId: string; modeId: string }>;
}

interface VariableEntry {
  variable: SceneVariable;
  collection: SceneVariableCollection;
}

function variableIndex(document: SceneDocument): Map<string, VariableEntry> {
  const index = new Map<string, VariableEntry>();
  for (const collection of document.variableCollections) {
    for (const variable of collection.variables) index.set(variable.id, { variable, collection });
  }
  return index;
}

function selectedMode(collection: SceneVariableCollection, selection: SceneVariableModeSelection): string {
  const modeId = selection[collection.id] ?? collection.modes[0].id;
  if (!collection.modes.some((mode) => mode.id === modeId)) throw new Error(`Variable collection ${collection.id} has no mode ${modeId}.`);
  return modeId;
}

export class SceneVariableResolver {
  private readonly variables: Map<string, VariableEntry>;
  private readonly nodes: ReturnType<typeof indexSceneDocument>;

  constructor(private readonly document: SceneDocument) {
    assertSceneDocument(document);
    this.variables = variableIndex(document);
    this.nodes = indexSceneDocument(document);
  }

  resolve(variableId: string, modeSelection: SceneVariableModeSelection = {}): ResolvedSceneVariable {
    const chain: ResolvedSceneVariable['chain'] = [];
    const resolving = new Set<string>();
    let currentId = variableId;
    while (true) {
      const entry = this.variables.get(currentId);
      if (!entry) throw new Error(`Scene variable not found: ${currentId}`);
      const modeId = selectedMode(entry.collection, modeSelection);
      const stateId = `${currentId}\u0000${modeId}`;
      if (resolving.has(stateId)) throw new Error(`Variable alias cycle while resolving ${variableId}.`);
      resolving.add(stateId);
      chain.push({ variableId: currentId, collectionId: entry.collection.id, modeId });
      if (Object.hasOwn(entry.variable.valuesByMode, modeId)) {
        return {
          variableId,
          collectionId: entry.collection.id,
          modeId,
          type: entry.variable.type,
          value: structuredClone(entry.variable.valuesByMode[modeId]),
          chain
        };
      }
      currentId = entry.variable.aliasByMode![modeId];
    }
  }

  resolveNode(nodeId: string, modeSelection: SceneVariableModeSelection = {}): Record<string, ResolvedSceneVariable> {
    const node = this.nodes.get(nodeId)?.node;
    if (!node) throw new Error(`Scene node not found: ${nodeId}`);
    return Object.fromEntries(Object.entries(node.variableBindings).map(([propertyPath, variableId]) => [
      propertyPath,
      this.resolve(variableId, modeSelection)
    ]));
  }
}

export function resolveSceneVariable(
  document: SceneDocument,
  variableId: string,
  modeSelection: SceneVariableModeSelection = {}
): ResolvedSceneVariable {
  return new SceneVariableResolver(document).resolve(variableId, modeSelection);
}

export function resolveSceneNodeVariableBindings(
  document: SceneDocument,
  nodeId: string,
  modeSelection: SceneVariableModeSelection = {}
): Record<string, ResolvedSceneVariable> {
  return new SceneVariableResolver(document).resolveNode(nodeId, modeSelection);
}
