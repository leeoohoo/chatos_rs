import { invoke } from "@tauri-apps/api/core";
import { messageTaskGraphSchema } from "../../../contracts/conversationRunContract";
import { runInspectorDetailSchema } from "../domain/runInspector";
import type { RunInspectorPort } from "../ports/RunInspectorPort";

export class TauriRunInspectorAdapter implements RunInspectorPort {
  async loadGraph(lookup: Parameters<RunInspectorPort["loadGraph"]>[0]) {
    const value = await invoke("load_message_task_graph", { input: lookup });
    return value == null ? null : messageTaskGraphSchema.parse(value);
  }

  async loadRun(ownerUserId: string, runId: string, eventOffset: number) {
    return runInspectorDetailSchema.parse(
      await invoke("load_run_inspector", { ownerUserId, runId, eventOffset }),
    );
  }
}
