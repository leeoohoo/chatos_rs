import type { RunInspectorDetail, RunInspectorGraph } from "../domain/runInspector";

export interface RunInspectorLookup {
  ownerUserId: string;
  conversationId: string;
  turnId: string;
  userMessageId: string;
}

export interface RunInspectorPort {
  loadGraph(lookup: RunInspectorLookup): Promise<RunInspectorGraph | null>;
  loadRun(ownerUserId: string, runId: string, eventOffset: number): Promise<RunInspectorDetail>;
}
