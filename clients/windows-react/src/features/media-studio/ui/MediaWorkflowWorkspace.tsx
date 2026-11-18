import { useEffect } from "react";
import {
  mediaWorkflowImageSizes,
  mediaWorkflowNodeLabel,
  mediaWorkflowVideoRatios,
  mediaWorkflowVideoSeconds,
  mediaWorkflowVideoSizes,
  type MediaWorkflowNodeKind,
} from "../domain/mediaWorkflow";
import { useMediaWorkflowStore } from "../application/mediaWorkflowStore";
import type { MediaWorkflowNodeState } from "../application/mediaWorkflowStore";
import { useMediaStudioStore } from "../application/mediaStudioStore";
import type { MediaStudioPort } from "../ports/MediaStudioPort";
import type { MediaStudioModel } from "../domain/mediaStudio";

export function MediaWorkflowWorkspace({ ownerUserId, port }: { ownerUserId: string; port: MediaStudioPort }) {
  const state = useMediaWorkflowStore();
  const models = useMediaStudioStore((value) => value.models);

  useEffect(() => {
    void state.activate(port, ownerUserId);
    return () => state.reset();
  }, [ownerUserId, port, state.activate, state.reset]);

  if (state.draft) {
    return <WorkflowEditor models={models} port={port} />;
  }
  return <WorkflowList models={models} />;
}

function WorkflowList({ models }: { models: MediaStudioModel[] }) {
  const state = useMediaWorkflowStore();
  const initialModel = models.find((model) => model.workload === "image")
    ?? models.find((model) => model.workload === "chat");
  const initialKind: MediaWorkflowNodeKind = initialModel?.workload === "chat" ? "text" : "image";
  const create = () => initialModel && state.create(initialModel.id, initialKind);
  return (
    <div className="media-workflow">
      <header className="media-workflow__toolbar">
        <div><h2>工作流</h2><p>把文本、图片和视频模型串成可重复运行的创作流程。</p></div>
        <button className="media-primary-button" disabled={!initialModel} onClick={create} type="button">＋ 新建工作流</button>
      </header>
      {state.isLoading ? <div className="media-workflow__empty"><span className="mini-spinner" />正在读取工作流…</div> : state.workflows.length === 0 ? (
        <div className="media-workflow__empty"><strong>还没有工作流</strong><span>{initialModel ? "新建一个流程，从输入开始连接文本、图片或视频节点。" : "请先配置可用的文本或图片模型。"}</span><button disabled={!initialModel} onClick={create} type="button">新建工作流</button></div>
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
  models: MediaStudioModel[];
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
        <button disabled={state.isSaving || state.isRunning} onClick={() => void state.remove(port)} type="button">删除</button>
        <button disabled={state.isSaving || state.isRunning} onClick={() => void state.run(port)} type="button">{state.isRunning ? "运行中…" : "运行"}</button>
        <button className="media-primary-button" disabled={state.isSaving || state.isRunning} onClick={() => void state.save(port)} type="button">{state.isSaving ? "保存中…" : "保存"}</button>
      </header>
      <div className="media-workflow__editor-grid">
        <section className="media-workflow__canvas" aria-label="工作流节点">
          {draft.nodes.map((node, index) => (
            <div className="media-workflow__node" key={node.id}>
              <span className={`media-workflow__node-kind media-workflow__node-kind--${node.kind}`}>{mediaWorkflowNodeLabel(node.kind)}</span>
              <div className="media-workflow__node-heading"><strong>{index + 1}. {mediaWorkflowNodeLabel(node.kind)}节点</strong><small>{node.modelConfigId || "未选择模型"}</small></div>
              <WorkflowNodeBadge state={state.nodeStates[node.id]} />
              <button aria-label={`上移第 ${index + 1} 个节点`} disabled={state.isRunning || state.isSaving || index === 0} onClick={() => state.moveNode(node.id, -1)} type="button">↑</button>
              <button aria-label={`下移第 ${index + 1} 个节点`} disabled={state.isRunning || state.isSaving || index === draft.nodes.length - 1} onClick={() => state.moveNode(node.id, 1)} type="button">↓</button>
              <button aria-label={`删除第 ${index + 1} 个节点`} disabled={state.isRunning || state.isSaving || draft.nodes.length <= 1} onClick={() => state.removeNode(node.id)} type="button">×</button>
              <textarea aria-label={`${mediaWorkflowNodeLabel(node.kind)}节点提示词`} disabled={state.isRunning || state.isSaving} maxLength={20_000} onChange={(event) => state.updateDraft((workflow) => ({ ...workflow, nodes: workflow.nodes.map((value) => value.id === node.id ? { ...value, prompt: event.target.value } : value) }))} placeholder="节点提示词…" value={node.prompt} />
              <select aria-label={`${mediaWorkflowNodeLabel(node.kind)}节点模型`} disabled={state.isRunning || state.isSaving} onChange={(event) => state.updateDraft((workflow) => ({ ...workflow, nodes: workflow.nodes.map((value) => value.id === node.id ? { ...value, modelConfigId: event.target.value } : value) }))} value={node.modelConfigId}>
                <option value="">选择模型</option>{modelsForKind(models, node.kind).map((model) => <option key={model.id} value={model.id}>{model.name} · {model.modelName}</option>)}
              </select>
              {node.kind === "image" && (
                <label className="media-workflow__node-option">
                  <span>图片尺寸</span>
                  <select aria-label="图片节点尺寸" disabled={state.isRunning || state.isSaving} onChange={(event) => state.updateDraft((workflow) => ({ ...workflow, nodes: workflow.nodes.map((value) => value.id === node.id ? { ...value, imageSize: event.target.value as typeof value.imageSize } : value) }))} value={node.imageSize}>
                    {mediaWorkflowImageSizes.map((size) => <option key={size} value={size}>{size === "auto" ? "自动" : size}</option>)}
                  </select>
                </label>
              )}
              {node.kind === "video" && (
                <div className="media-workflow__node-options">
                  <label className="media-workflow__node-option">
                    <span>视频尺寸</span>
                    <select aria-label="视频节点尺寸" disabled={state.isRunning || state.isSaving} onChange={(event) => state.updateDraft((workflow) => ({ ...workflow, nodes: workflow.nodes.map((value) => value.id === node.id ? { ...value, videoSize: event.target.value as typeof value.videoSize } : value) }))} value={node.videoSize}>
                      {mediaWorkflowVideoSizes.map((size) => <option key={size}>{size}</option>)}
                    </select>
                  </label>
                  <label className="media-workflow__node-option">
                    <span>画面比例</span>
                    <select aria-label="视频节点比例" disabled={state.isRunning || state.isSaving} onChange={(event) => state.updateDraft((workflow) => ({ ...workflow, nodes: workflow.nodes.map((value) => value.id === node.id ? { ...value, videoRatio: event.target.value as typeof value.videoRatio } : value) }))} value={node.videoRatio}>
                      {mediaWorkflowVideoRatios.map((ratio) => <option key={ratio}>{ratio}</option>)}
                    </select>
                  </label>
                  <label className="media-workflow__node-option">
                    <span>时长</span>
                    <select aria-label="视频节点时长" disabled={state.isRunning || state.isSaving} onChange={(event) => state.updateDraft((workflow) => ({ ...workflow, nodes: workflow.nodes.map((value) => value.id === node.id ? { ...value, videoSeconds: Number(event.target.value) } : value) }))} value={node.videoSeconds}>
                      {mediaWorkflowVideoSeconds.map((seconds) => <option key={seconds} value={seconds}>{seconds} 秒</option>)}
                    </select>
                  </label>
                </div>
              )}
              <WorkflowNodeResult state={state.nodeStates[node.id]} />
            </div>
          ))}
          {canAddNode && <div className="media-workflow__add-node"><span>添加节点</span>{(["text", "image", "video"] as MediaWorkflowNodeKind[]).map((kind) => { const model = modelsForKind(models, kind)[0]; return <button disabled={!model} key={kind} onClick={() => model && state.addNode(kind, model.id)} type="button">＋ {mediaWorkflowNodeLabel(kind)}</button>; })}</div>}
          {!canAddNode && draft.nodes.length >= 8 && <small className="media-workflow__limit">最多支持 8 个节点。</small>}
        </section>
        <aside className="media-workflow__inspector"><h3>运行说明</h3><p>流程从第一个模型节点开始，按从上到下的顺序执行。视频节点只能出现一次并且必须是最后一个节点。</p><p className="media-workflow__notice">点击“运行”会先保存当前草稿，再串行调用文本、图片和视频节点；图片与视频结果会进入对应历史记录。</p>{state.runResult && <div className="media-workflow__run-result" role="status"><strong>运行完成</strong>{state.runResult.finalText && <p>{state.runResult.finalText}</p>}<small>图片记录 {state.runResult.imageRecordIds.length} 条 · 视频记录 {state.runResult.videoRecordIds.length} 条</small></div>}</aside>
      </div>
      {state.errorMessage && <WorkflowError />}
    </div>
  );
}

function modelsForKind(models: MediaStudioModel[], kind: MediaWorkflowNodeKind) {
  if (kind === "video") {
    return models.filter((model) => model.workload === "video" && model.modelName.toLocaleLowerCase("en-US") === "minimax-h3");
  }
  return models.filter((model) => model.workload === (kind === "text" ? "chat" : "image"));
}

function WorkflowNodeBadge({ state }: { state?: MediaWorkflowNodeState }) {
  if (!state) return null;
  if (state.phase === "running") return <span className="media-workflow__node-state media-workflow__node-state--running" role="status">{state.progress ?? "运行中"}</span>;
  if (state.phase === "completed") return <span aria-label="节点运行成功" className="media-workflow__node-state media-workflow__node-state--completed">✓</span>;
  return <span aria-label="节点运行失败" className="media-workflow__node-state media-workflow__node-state--failed">!</span>;
}

function WorkflowNodeResult({ state }: { state?: MediaWorkflowNodeState }) {
  if (!state) return null;
  if (state.phase === "running") {
    return <div className="media-workflow__node-result media-workflow__node-result--running" role="status"><span className="mini-spinner" />{state.progress ?? "正在运行节点…"}</div>;
  }
  if (state.phase === "failed") {
    return <div className="media-workflow__node-result media-workflow__node-result--failed" role="alert">{state.errorMessage}</div>;
  }
  if (state.result.type === "text") {
    return <div className="media-workflow__node-result"><strong>文本结果</strong><p>{state.result.text}</p></div>;
  }
  if (state.result.type === "images") {
    return (
      <div className="media-workflow__node-result">
        <strong>图片结果</strong>
        <div className="media-workflow__node-result-grid">
          {state.result.item.images.map((asset, index) => asset.previewUrl
            ? <img alt={`工作流节点生成图片 ${index + 1}`} key={asset.id} src={asset.previewUrl} />
            : <span key={asset.id}>图片预览暂不可用</span>)}
        </div>
      </div>
    );
  }
  return (
    <div className="media-workflow__node-result">
      <strong>视频结果</strong>
      {state.result.item.previewUrl
        ? <video aria-label="工作流节点生成视频" controls preload="metadata" src={state.result.item.previewUrl} />
        : <span>视频预览暂不可用</span>}
    </div>
  );
}

function WorkflowError() {
  const dismiss = useMediaWorkflowStore((state) => state.dismissError);
  return <div className="media-workflow__error" role="alert">{useMediaWorkflowStore.getState().errorMessage}<button aria-label="关闭工作流错误" onClick={dismiss} type="button">×</button></div>;
}
