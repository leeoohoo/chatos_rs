import { useEffect } from "react";
import {
  mediaWorkflowNodeLabel,
  type MediaWorkflowNodeKind,
} from "../domain/mediaWorkflow";
import { useMediaWorkflowStore } from "../application/mediaWorkflowStore";
import { useMediaStudioStore } from "../application/mediaStudioStore";
import type { MediaStudioPort } from "../ports/MediaStudioPort";

export function MediaWorkflowWorkspace({ ownerUserId, port }: { ownerUserId: string; port: MediaStudioPort }) {
  const state = useMediaWorkflowStore();
  const models = useMediaStudioStore((value) => value.models);

  useEffect(() => {
    void state.activate(port, ownerUserId);
    return () => state.reset();
  }, [ownerUserId, port, state.activate, state.reset]);

  if (state.draft) {
    return <WorkflowEditor models={models.map((model) => ({ id: model.id, label: `${model.name} · ${model.modelName}` }))} port={port} />;
  }
  return <WorkflowList models={models.map((model) => model.id)} />;
}

function WorkflowList({ models }: { models: string[] }) {
  const state = useMediaWorkflowStore();
  return (
    <div className="media-workflow">
      <header className="media-workflow__toolbar">
        <div><h2>工作流</h2><p>把文本、图片和视频模型串成可重复运行的创作流程。</p></div>
        <button className="media-primary-button" onClick={() => state.create(models[0])} type="button">＋ 新建工作流</button>
      </header>
      {state.isLoading ? <div className="media-workflow__empty"><span className="mini-spinner" />正在读取工作流…</div> : state.workflows.length === 0 ? (
        <div className="media-workflow__empty"><strong>还没有工作流</strong><span>新建一个流程，从输入开始连接文本、图片或视频节点。</span><button onClick={() => state.create(models[0])} type="button">新建工作流</button></div>
      ) : (
        <div className="media-workflow__grid">
          {state.workflows.map((workflow) => (
            <button className="media-workflow__card" key={workflow.id} onClick={() => state.open(workflow)} type="button">
              <strong>{workflow.name}</strong>
              <span>{workflow.nodes.map((node) => mediaWorkflowNodeLabel(node.kind)).join(" → ")}</span>
              <small>{workflow.nodes.length} 个节点 · {new Date(workflow.updatedAtUnixMs).toLocaleString()}</small>
            </button>
          ))}
        </div>
      )}
      {state.errorMessage && <WorkflowError />}
    </div>
  );
}

function WorkflowEditor({
  models,
  port,
}: {
  models: Array<{ id: string; label: string }>;
  port: MediaStudioPort;
}) {
  const state = useMediaWorkflowStore();
  const draft = state.draft!;
  const canAddNode = draft.nodes.length < 8 && !draft.nodes.some((node) => node.kind === "video");
  return (
    <div className="media-workflow media-workflow--editor">
      <header className="media-workflow__toolbar">
        <button aria-label="返回工作流列表" onClick={state.close} type="button">‹</button>
        <input aria-label="工作流名称" maxLength={128} onChange={(event) => state.updateDraft((workflow) => ({ ...workflow, name: event.target.value }))} value={draft.name} />
        <span className="media-workflow__toolbar-spacer" />
        <button disabled={state.isSaving} onClick={() => void state.remove(port)} type="button">删除</button>
        <button className="media-primary-button" disabled={state.isSaving} onClick={() => void state.save(port)} type="button">{state.isSaving ? "保存中…" : "保存"}</button>
      </header>
      <div className="media-workflow__editor-grid">
        <section className="media-workflow__canvas" aria-label="工作流节点">
          <div className="media-workflow__input-card"><strong>流程输入</strong><textarea aria-label="流程输入提示词" maxLength={20_000} onChange={(event) => state.updateDraft((workflow) => ({ ...workflow, inputPrompt: event.target.value }))} placeholder="输入整个流程的初始要求…" value={draft.inputPrompt} /></div>
          {draft.nodes.map((node, index) => (
            <div className="media-workflow__node" key={node.id}>
              <span className={`media-workflow__node-kind media-workflow__node-kind--${node.kind}`}>{mediaWorkflowNodeLabel(node.kind)}</span>
              <div><strong>{index + 1}. {mediaWorkflowNodeLabel(node.kind)}节点</strong><small>{node.modelConfigId || "未选择模型"}</small></div>
              <button aria-label={`上移第 ${index + 1} 个节点`} disabled={index === 0} onClick={() => state.moveNode(node.id, -1)} type="button">↑</button>
              <button aria-label={`下移第 ${index + 1} 个节点`} disabled={index === draft.nodes.length - 1} onClick={() => state.moveNode(node.id, 1)} type="button">↓</button>
              <button aria-label={`删除第 ${index + 1} 个节点`} disabled={draft.nodes.length <= 1} onClick={() => state.removeNode(node.id)} type="button">×</button>
              <textarea aria-label={`${mediaWorkflowNodeLabel(node.kind)}节点提示词`} maxLength={20_000} onChange={(event) => state.updateDraft((workflow) => ({ ...workflow, nodes: workflow.nodes.map((value) => value.id === node.id ? { ...value, prompt: event.target.value } : value) }))} placeholder="节点提示词…" value={node.prompt} />
              <select aria-label={`${mediaWorkflowNodeLabel(node.kind)}节点模型`} onChange={(event) => state.updateDraft((workflow) => ({ ...workflow, nodes: workflow.nodes.map((value) => value.id === node.id ? { ...value, modelConfigId: event.target.value } : value) }))} value={node.modelConfigId}>
                <option value="">选择模型</option>{models.map((model) => <option key={model.id} value={model.id}>{model.label}</option>)}
              </select>
            </div>
          ))}
          {canAddNode && <div className="media-workflow__add-node"><span>添加节点</span>{(["text", "image", "video"] as MediaWorkflowNodeKind[]).map((kind) => <button key={kind} onClick={() => state.addNode(kind, models[0]?.id)} type="button">＋ {mediaWorkflowNodeLabel(kind)}</button>)}</div>}
          {!canAddNode && draft.nodes.length >= 8 && <small className="media-workflow__limit">最多支持 8 个节点。</small>}
        </section>
        <aside className="media-workflow__inspector"><h3>运行说明</h3><p>节点按从上到下的顺序执行。视频节点只能出现一次并且必须是最后一个节点。</p><p className="media-workflow__notice">工作流保存和账号隔离已可用；执行器会在下一段接入现有图片/视频持久任务。</p></aside>
      </div>
      {state.errorMessage && <WorkflowError />}
    </div>
  );
}

function WorkflowError() {
  const dismiss = useMediaWorkflowStore((state) => state.dismissError);
  return <div className="media-workflow__error" role="alert">{useMediaWorkflowStore.getState().errorMessage}<button aria-label="关闭工作流错误" onClick={dismiss} type="button">×</button></div>;
}
