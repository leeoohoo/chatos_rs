import { useEffect, useLayoutEffect } from "react";
import { useProjectCreationStore } from "../application/projectCreationStore";
import { canSubmitProject, type CreatedLocalProject } from "../domain/projectCreation";
import type { ProjectCreationPort } from "../ports/ProjectCreationPort";

export function CreateProjectSheet({
  ownerUserId,
  port,
  onDismiss,
  onCreated,
}: {
  ownerUserId: string;
  port: ProjectCreationPort;
  onDismiss(): void;
  onCreated(project: CreatedLocalProject): void;
}) {
  const state = useProjectCreationStore();

  useLayoutEffect(() => {
    void useProjectCreationStore.getState().open(port, ownerUserId);
    return () => useProjectCreationStore.getState().close(port);
  }, [ownerUserId, port]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !useProjectCreationStore.getState().isSaving) onDismiss();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onDismiss]);

  const selectedType = state.projectTypes.find((item) => item.key === state.projectTypeKey);
  const canCreate = canSubmitProject(state);
  const create = async () => {
    const project = await useProjectCreationStore.getState().save(port);
    if (!project) return;
    onDismiss();
    queueMicrotask(() => onCreated(project));
  };

  return (
    <div className="project-sheet-backdrop" role="presentation">
      <section aria-labelledby="create-project-title" aria-modal="true" className="project-sheet" role="dialog">
        <header className="project-sheet__header">
          <span aria-hidden="true">＋</span>
          <div>
            <h2 id="create-project-title">新建项目</h2>
            <p>项目只保存在本机；不会上传代码、创建 Git remote 或远端项目。</p>
          </div>
        </header>

        <div className="project-sheet__body">
          <section className="project-folder-picker" aria-label="项目根目录">
            <div className="project-folder-picker__icon" aria-hidden="true">▰</div>
            <div>
              <strong>{state.selection?.directoryName ?? "选择项目根目录"}</strong>
              <small>{state.selection
                ? "该文件夹本身会成为项目根目录。路径仅保留在原生授权层。"
                : "请选择一个已有文件夹；创建项目不会自动建立同名子文件夹。"}</small>
            </div>
            <button
              disabled={state.isPicking || state.isSaving}
              onClick={() => void state.pickDirectory(port)}
              type="button"
            >
              {state.isPicking ? <><span className="mini-spinner" />正在选择…</> : state.selection ? "重新选择" : "选择文件夹…"}
            </button>
          </section>

          <label className="project-form-field">
            <span>项目名称</span>
            <input
              autoFocus
              disabled={state.isSaving}
              maxLength={1024}
              onChange={(event) => state.setProjectName(event.target.value)}
              placeholder="项目名称"
              value={state.projectName}
            />
          </label>

          <label className="project-form-field">
            <span>项目说明 <small>可选</small></span>
            <textarea
              disabled={state.isSaving}
              maxLength={16384}
              onChange={(event) => state.setProjectDescription(event.target.value)}
              placeholder="简要描述项目目标"
              rows={3}
              value={state.projectDescription}
            />
          </label>

          <label className="project-form-field">
            <span>项目类型</span>
            <select
              disabled={state.isLoadingTypes || state.isSaving}
              onChange={(event) => state.setProjectTypeKey(event.target.value)}
              value={state.projectTypeKey}
            >
              {state.projectTypes.map((projectType) => (
                <option key={projectType.key} value={projectType.key}>
                  {projectType.categoryLabel} · {projectType.label}
                </option>
              ))}
            </select>
            {selectedType && <small className="project-type-description">{selectedType.description}</small>}
          </label>

          <p className="project-local-note"><span aria-hidden="true">⌂</span>项目注册表按当前账号隔离，授权只覆盖你明确选择的文件夹。</p>
          {state.errorMessage && <p className="project-sheet__error" role="alert"><span aria-hidden="true">!</span>{state.errorMessage}</p>}
        </div>

        <footer className="project-sheet__footer">
          <button disabled={state.isSaving} onClick={onDismiss} type="button">取消</button>
          <button className="primary-button" disabled={!canCreate} onClick={() => void create()} type="button">
            {state.isSaving ? <><span className="button-spinner" />正在创建…</> : "创建项目"}
          </button>
        </footer>
      </section>
    </div>
  );
}
