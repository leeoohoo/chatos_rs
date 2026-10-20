import type {
  RunInspectorDetail,
  RunInspectorGraph,
  TaskInspectorDetail,
  TaskMutationReceipt,
} from "../domain/runInspector";

export interface RunInspectorLookup {
  ownerUserId: string;
  conversationId: string;
  turnId: string;
  userMessageId: string;
}

export interface TaskMutationRequest {
  ownerUserId: string;
  taskId: string;
  expectedVersion: number;
  instruction?: string;
  reason?: string;
}

export interface RunInspectorPort {
  loadGraph(lookup: RunInspectorLookup): Promise<RunInspectorGraph | null>;
  loadTask(lookup: RunInspectorLookup, taskId: string): Promise<TaskInspectorDetail>;
  loadRun(ownerUserId: string, runId: string, eventOffset: number): Promise<RunInspectorDetail>;
  retryTask(request: TaskMutationRequest): Promise<TaskMutationReceipt>;
  cancelTask(request: TaskMutationRequest): Promise<TaskMutationReceipt>;
  restartTask(request: TaskMutationRequest): Promise<TaskMutationReceipt>;
}
