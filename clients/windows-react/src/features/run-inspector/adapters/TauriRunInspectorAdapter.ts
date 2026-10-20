import { invoke } from "@tauri-apps/api/core";
import { messageTaskGraphSchema } from "../../../contracts/conversationRunContract";
import {
  runInspectorDetailSchema,
  taskInspectorDetailSchema,
  taskMutationReceiptSchema,
} from "../domain/runInspector";
import type { RunInspectorPort, TaskMutationRequest } from "../ports/RunInspectorPort";

export class TauriRunInspectorAdapter implements RunInspectorPort {
  async loadGraph(lookup: Parameters<RunInspectorPort["loadGraph"]>[0]) {
    const value = await invoke("load_message_task_graph", { input: lookup });
    return value == null ? null : messageTaskGraphSchema.parse(value);
  }

  async loadTask(
    lookup: Parameters<RunInspectorPort["loadTask"]>[0],
    taskId: string,
  ) {
    return taskInspectorDetailSchema.parse(
      await invoke("load_task_inspector", { input: { ...lookup, taskId } }),
    );
  }

  async loadRun(ownerUserId: string, runId: string, eventOffset: number) {
    return runInspectorDetailSchema.parse(
      await invoke("load_run_inspector", { ownerUserId, runId, eventOffset }),
    );
  }

  async retryTask(request: TaskMutationRequest) {
    return this.mutate("retry_task_inspector", request);
  }

  async cancelTask(request: TaskMutationRequest) {
    return this.mutate("cancel_task_inspector", request);
  }

  async restartTask(request: TaskMutationRequest) {
    return this.mutate("restart_task_inspector", request);
  }

  private async mutate(command: string, request: TaskMutationRequest) {
    return taskMutationReceiptSchema.parse(await invoke(command, { input: request }));
  }
}
