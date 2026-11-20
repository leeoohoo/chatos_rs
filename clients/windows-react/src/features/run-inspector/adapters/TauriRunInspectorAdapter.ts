import { invoke } from "@tauri-apps/api/core";
import {
  runInspectorGraphSchema,
  runInspectorDetailSchema,
  taskInspectorDetailSchema,
  taskMutationReceiptSchema,
} from "../domain/runInspector";
import type { RunInspectorPort, TaskMutationRequest } from "../ports/RunInspectorPort";

export class TauriRunInspectorAdapter implements RunInspectorPort {
  async loadGraph(lookup: Parameters<RunInspectorPort["loadGraph"]>[0]) {
    const value = await invoke("load_message_task_graph", { input: lookup });
    return value == null ? null : runInspectorGraphSchema.parse(value);
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
    try {
      return runInspectorDetailSchema.parse(
        await invoke("load_run_inspector", { ownerUserId, runId, eventOffset }),
      );
    } catch (failure) {
      throw new Error(readInvokeFailure(failure, "Run 详情暂时不可用。"));
    }
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

function readInvokeFailure(failure: unknown, fallback: string): string {
  if (failure instanceof Error && failure.message.trim()) return failure.message;
  if (typeof failure === "string" && failure.trim()) return failure;
  if (failure && typeof failure === "object") {
    const value = failure as Record<string, unknown>;
    for (const key of ["message", "error", "detail"]) {
      const message = value[key];
      if (typeof message === "string" && message.trim()) return message;
    }
  }
  return fallback;
}
