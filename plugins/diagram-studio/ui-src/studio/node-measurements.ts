export interface RuntimeNodeMeasurement {
  width: number;
  height: number;
}

export interface NodeDimensionChangeLike {
  id?: string;
  type: string;
  dimensions?: RuntimeNodeMeasurement;
}

export type NodeMeasurementCache = Map<string, RuntimeNodeMeasurement>;

export function rememberNodeMeasurements(
  cache: NodeMeasurementCache,
  documentId: string,
  changes: readonly NodeDimensionChangeLike[]
): void {
  for (const change of changes) {
    if (change.type !== 'dimensions' || !change.id || !change.dimensions) continue;
    cache.set(nodeMeasurementKey(documentId, change.id), { ...change.dimensions });
  }
}

export function measuredNode<T extends { id: string }>(
  cache: NodeMeasurementCache,
  documentId: string,
  node: T
): T & { measured?: RuntimeNodeMeasurement } {
  const measured = cache.get(nodeMeasurementKey(documentId, node.id));
  return measured ? { ...node, measured } : node;
}

function nodeMeasurementKey(documentId: string, nodeId: string): string {
  return `${documentId}\u0000${nodeId}`;
}
