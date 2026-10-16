import { useEffect, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import type { DesignContentBlock, DesignContentType, DesignSection, ExecutionTask, PlanNodePosition, RequirementItem, RequirementModuleKind, RequirementPriority, SolutionWorkspace, SolutionWorkspaceSummary, TaskStatus, WorkspaceValidation } from '../../src/schema';
import { createRepository, type RuntimeContext, type SolutionRepository } from './repository';
import { PlanGraph } from './PlanGraph';
import { readyTaskIds, requirementPlanView } from '../../src/plan-view';

type Area = 'overview' | 'requirements' | 'plan';
type DirtyPart = 'requirements' | 'design' | 'plan' | 'workspace';
type RequirementPanelTab = 'detail' | 'design' | 'plan';
type UiScale = 'compact' | 'comfortable' | 'large';
type CreateModalState = { kind: 'module'; moduleId?: string } | { kind: 'requirement'; moduleId?: string } | { kind: 'design'; requirementId: string } | { kind: 'designBlock'; requirementId: string; designId: string } | { kind: 'task'; requirementId: string };

const navigation: Array<{ id: Area; icon: string; label: string; caption: string }> = [
  { id: 'overview', icon: '⌂', label: '项目概览', caption: '状态与追踪' },
  { id: 'requirements', icon: '◎', label: '功能模块', caption: '需求与设计' },
  { id: 'plan', icon: '⌘', label: '任务图', caption: '模块执行顺序' }
];

const taskStatusLabels: Record<TaskStatus, string> = { planned: '未开始', in_progress: '进行中', blocked: '阻塞', done: '完成', cancelled: '取消' };
const designTypeLabels: Record<DesignContentType, string> = { text: '文本', architecture: '架构图', flowchart: '流程图', 'ui-svg': '页面设计图' };
const requirementPriorityLabels: Record<RequirementPriority, string> = { must: '高', should: '中', could: '低' };

function lines(value: string) { return value.split('\n').map((item) => item.trim()).filter(Boolean); }
function timestamp() { return new Date().toISOString(); }

export function SolutionStudioApp() {
  const [repository, setRepository] = useState<SolutionRepository>();
  const [context, setContext] = useState<RuntimeContext>();
  const [items, setItems] = useState<SolutionWorkspaceSummary[]>([]);
  const [workspace, setWorkspace] = useState<SolutionWorkspace>();
  const [persistedRevision, setPersistedRevision] = useState(0);
  const [area, setArea] = useState<Area>('overview');
  const [dirtyParts, setDirtyParts] = useState<Set<DirtyPart>>(() => new Set());
  const [saving, setSaving] = useState(false);
  const [validation, setValidation] = useState<WorkspaceValidation>();
  const [selectedTaskId, setSelectedTaskId] = useState<string>();
  const [activeRequirementId, setActiveRequirementId] = useState<string>();
  const [requirementPanelTab, setRequirementPanelTab] = useState<RequirementPanelTab>('design');
  const [createModal, setCreateModal] = useState<CreateModalState>();
  const [toast, setToast] = useState<string>();
  const [creating, setCreating] = useState(false);
  const [newTitle, setNewTitle] = useState('');
  const [openingWorkspaceId, setOpeningWorkspaceId] = useState<string>();
  const [markdownPreview, setMarkdownPreview] = useState<{ title: string; content: string }>();
  const [loadingMarkdown, setLoadingMarkdown] = useState(false);
  const [uiScale, setUiScale] = useState<UiScale>(() => {
    const saved = window.localStorage.getItem('solution-studio-ui-scale');
    return saved === 'compact' || saved === 'large' ? saved : 'comfortable';
  });

  useEffect(() => {
    void (async () => {
      const repo = await createRepository();
      const runtime = await repo.context();
      const workspaces = await repo.list();
      setRepository(repo);
      setContext(runtime);
      setNewTitle(runtime.projectName ? `${runtime.projectName} 方案` : '新项目方案');
      setItems(workspaces);
    })().catch((error) => notify(error instanceof Error ? error.message : String(error)));
  }, []);

  useEffect(() => {
    if (!repository || !context || workspace || openingWorkspaceId || items.length === 0) return;
    const workspaceId = items[0].workspaceId;
    setOpeningWorkspaceId(workspaceId);
    void openWorkspace(repository, workspaceId)
      .catch((error) => {
        notify(error instanceof Error ? error.message : String(error));
      })
      .finally(() => setOpeningWorkspaceId(undefined));
  }, [context, items, openingWorkspaceId, repository, workspace]);

  useEffect(() => {
    if (!repository || repository.mode !== 'server') return;
    const events = new EventSource('/api/events');
    events.addEventListener('workspaces-changed', () => void repository.list().then(setItems));
    return () => events.close();
  }, [repository]);

  useEffect(() => {
    window.localStorage.setItem('solution-studio-ui-scale', uiScale);
  }, [uiScale]);

  function notify(message: string) {
    setToast(message);
    window.setTimeout(() => setToast((current) => current === message ? undefined : current), 2800);
  }

  async function openWorkspace(repo: SolutionRepository, workspaceId: string) {
    const next = await repo.read(workspaceId);
    setWorkspace(next);
    setPersistedRevision(next.revision);
    setDirtyParts(new Set());
    setSelectedTaskId(undefined);
    setActiveRequirementId(undefined);
    setCreateModal(undefined);
    setArea('overview');
    if (repo.mode === 'server') setValidation(await repo.validate(workspaceId));
  }

  async function createWorkspace() {
    if (!repository || !context || !newTitle.trim()) return;
    setCreating(true);
    try {
      const created = await repository.create(newTitle.trim(), context.sourceModeHint);
      setItems(await repository.list());
      await openWorkspace(repository, created.workspaceId);
    } catch (error) { notify(error instanceof Error ? error.message : String(error)); }
    finally { setCreating(false); }
  }

  function updateWorkspace(part: DirtyPart, updater: (draft: SolutionWorkspace) => void) {
    setWorkspace((current) => {
      if (!current) return current;
      const next = structuredClone(current);
      updater(next);
      setDirtyParts((parts) => new Set([...parts, part]));
      return next;
    });
  }

  async function save() {
    if (!workspace || !repository || dirtyParts.size === 0 || saving) return;
    setSaving(true);
    try {
      const candidate = structuredClone(workspace);
      const now = timestamp();
      if (dirtyParts.has('requirements')) { candidate.requirements.revision += 1; candidate.requirements.updatedAt = now; }
      if (dirtyParts.has('design')) { candidate.design.revision += 1; candidate.design.basedOnRequirementsRevision = candidate.requirements.revision; candidate.design.updatedAt = now; }
      if (dirtyParts.has('plan')) { candidate.executionPlan.revision += 1; candidate.executionPlan.basedOnDesignRevision = candidate.design.revision; candidate.executionPlan.updatedAt = now; }
      const saved = await repository.save(candidate, persistedRevision);
      setWorkspace(saved);
      setPersistedRevision(saved.revision);
      setDirtyParts(new Set());
      setItems(await repository.list());
      if (repository.mode === 'server') setValidation(await repository.validate(saved.workspaceId));
      notify('方案已保存');
    } catch (error) { notify(error instanceof Error ? error.message : String(error)); }
    finally { setSaving(false); }
  }

  async function openMarkdownPreview() {
    if (!workspace || !repository || loadingMarkdown) return;
    setLoadingMarkdown(true);
    try {
      setMarkdownPreview({ title: workspace.title, content: await repository.markdown(workspace.workspaceId) });
    } catch (error) { notify(error instanceof Error ? error.message : String(error)); }
    finally { setLoadingMarkdown(false); }
  }

  if (!repository || !context) return <div className="launch-screen"><LogoMark/><span className="spinner"/><strong>正在准备 Solution Studio…</strong></div>;

  if (!workspace && items.length > 0) return <div className="launch-screen"><LogoMark/><span className="spinner"/><strong>正在打开项目规划…</strong></div>;

  if (!workspace) return <Landing context={context} title={newTitle} creating={creating} onTitle={setNewTitle} onCreate={() => void createWorkspace()} />;

  const currentWorkspace = workspace;
  const activeRequirement = workspace.requirements.items.find((item) => item.id === activeRequirementId);
  const requirementsWithDesign = new Set(workspace.design.sections.flatMap((section) => section.requirementIds)).size;
  const requirementsWithTasks = new Set(workspace.executionPlan.tasks.flatMap((task) => task.requirementIds)).size;
  const doneCount = workspace.executionPlan.tasks.filter((task) => task.status === 'done').length;
  const readyCount = readyTaskIds(workspace.executionPlan.tasks).length;

  function saveModule(input: { moduleId?: string; title: string; description: string; kind: RequirementModuleKind }) {
    let id = input.moduleId ?? '';
    updateWorkspace('requirements', (draft) => {
      if (input.moduleId) {
        const module = draft.requirements.modules.find((candidate) => candidate.id === input.moduleId);
        if (!module) throw new Error('模块不存在。');
        module.title = input.title;
        module.description = input.description;
        module.kind = input.kind;
        return;
      }
      const number = Math.max(0, ...draft.requirements.modules.map((item) => Number.parseInt(item.id.replace(/\D/g, ''), 10) || 0)) + 1;
      id = `MOD-${String(number).padStart(3, '0')}`;
      draft.requirements.modules.push({ id, title: input.title, description: input.description, kind: input.kind, order: draft.requirements.modules.length });
    });
    setCreateModal(undefined);
    notify(input.moduleId ? `已更新 ${id}` : `已创建 ${id}`);
  }

  function deleteModule(moduleId: string) {
    const used = currentWorkspace.requirements.items.some((item) => item.moduleId === moduleId)
      || currentWorkspace.design.sections.some((section) => section.moduleId === moduleId)
      || currentWorkspace.executionPlan.tasks.some((task) => task.moduleId === moduleId);
    if (used) { notify('请先删除或移动该模块下的需求、设计和任务。'); return; }
    updateWorkspace('requirements', (draft) => {
      draft.requirements.modules = draft.requirements.modules.filter((module) => module.id !== moduleId).map((module, order) => ({ ...module, order }));
    });
    notify(`已删除 ${moduleId}`);
  }

  function addRequirement(input: { moduleId: string; title: string; description: string; priority: 'must' | 'should' | 'could'; parentRequirementId?: string; acceptanceCriteria: string[] }) {
    let id = '';
    updateWorkspace('requirements', (draft) => {
      const number = Math.max(0, ...draft.requirements.items.map((item) => Number.parseInt(item.id.replace(/\D/g, ''), 10) || 0)) + 1;
      id = `R-${String(number).padStart(3, '0')}`;
      const module = draft.requirements.modules.find((candidate) => candidate.id === input.moduleId);
      if (!module) throw new Error('请先选择有效功能模块。');
      const parent = input.parentRequirementId ? draft.requirements.items.find((item) => item.id === input.parentRequirementId) : undefined;
      if (parent && parent.moduleId !== module.id) throw new Error('父需求必须与子需求位于同一模块。');
      draft.requirements.items.push({ id, moduleId: module.id, kind: 'capability', ...(input.parentRequirementId ? { parentRequirementId: input.parentRequirementId } : {}), title: input.title, description: input.description, priority: input.priority, acceptanceCriteria: input.acceptanceCriteria, evidenceIds: [] });
    });
    setCreateModal(undefined);
    notify(`已创建 ${id}`);
  }

  function addDesignSection(requirementId: string, input: { title: string; body: string }) {
    if (currentWorkspace.design.sections.some((section) => section.requirementIds.includes(requirementId))) {
      notify('每条需求只能有一个设计方案');
      return;
    }
    const number = Math.max(0, ...currentWorkspace.design.sections.map((item) => Number.parseInt(item.id.replace(/\D/g, ''), 10) || 0)) + 1;
    const id = `D-${String(number).padStart(3, '0')}`;
    updateWorkspace('design', (draft) => {
      const moduleId = draft.requirements.items.find((item) => item.id === requirementId)?.moduleId;
      if (!moduleId) throw new Error('需求没有所属模块。');
      draft.design.sections.push({ id, moduleId, title: input.title, body: input.body, requirementIds: [requirementId], evidenceIds: [], blocks: [{ id: `${id}-B-001`, type: 'text', title: '方案说明', content: input.body }] });
      draft.design.basedOnRequirementsRevision = draft.requirements.revision;
    });
    updateWorkspace('requirements', (draft) => {
      const target = draft.requirements.items.find((item) => item.id === requirementId);
      if (target) target.selectedDesignSectionId = id;
    });
    setCreateModal(undefined);
    notify(`已为 ${requirementId} 创建设计方案 ${id}`);
  }

  function addDesignBlock(designId: string, input: { type: DesignContentType; title: string; content: string }) {
    let id = '';
    updateWorkspace('design', (draft) => {
      const section = draft.design.sections.find((item) => item.id === designId);
      if (!section) return;
      const number = Math.max(0, ...draft.design.sections.flatMap((item) => item.blocks ?? []).map((item) => Number.parseInt(item.id.match(/B-(\d+)$/)?.[1] ?? '0', 10))) + 1;
      id = `${designId}-B-${String(number).padStart(3, '0')}`;
      section.blocks = [...(section.blocks ?? []), { id, ...input }];
    });
    setCreateModal(undefined);
    notify(`已添加${designTypeLabels[input.type]}`);
  }

  function addTask(requirementId: string, input: { title: string; description: string; phase: string; acceptanceCriteria: string[]; dependsOn: string[] }) {
    const requirement = currentWorkspace.requirements.items.find((item) => item.id === requirementId);
    const selectedDesignSectionId = requirement?.selectedDesignSectionId;
    if (!requirement) return;
    updateWorkspace('plan', (draft) => {
      const number = Math.max(0, ...draft.executionPlan.tasks.map((item) => Number.parseInt(item.id.replace(/\D/g, ''), 10) || 0)) + 1;
      const id = `T-${String(number).padStart(3, '0')}`;
      draft.executionPlan.tasks.push({ id, moduleId: requirement.moduleId, title: input.title, description: input.description, type: 'task', phase: input.phase, dependsOn: input.dependsOn, status: 'planned', requirementIds: [requirementId], designSectionIds: selectedDesignSectionId ? [selectedDesignSectionId] : [], deliverables: [], acceptanceCriteria: input.acceptanceCriteria, sourceReferences: [] });
      draft.executionPlan.basedOnDesignRevision = draft.design.revision;
      setSelectedTaskId(id);
    });
    setCreateModal(undefined);
    notify(`已为 ${requirementId} 添加执行任务`);
  }

  function openRequirement(requirementId: string, tab: RequirementPanelTab) {
    setActiveRequirementId(requirementId);
    setRequirementPanelTab(tab);
    setSelectedTaskId(undefined);
  }

  function updateTask(taskId: string, updater: (task: ExecutionTask) => void) {
    updateWorkspace('plan', (draft) => {
      const task = draft.executionPlan.tasks.find((candidate) => candidate.id === taskId);
      if (task) updater(task);
    });
  }

  function deleteTask(taskId: string) {
    const task = currentWorkspace.executionPlan.tasks.find((candidate) => candidate.id === taskId);
    if (!task || task.status !== 'planned') { notify('只能删除尚未领取的任务。'); return; }
    if (currentWorkspace.executionPlan.tasks.some((candidate) => candidate.dependsOn.includes(taskId))) { notify('请先移除其他任务对它的依赖。'); return; }
    updateWorkspace('plan', (draft) => {
      draft.executionPlan.tasks = draft.executionPlan.tasks.filter((candidate) => candidate.id !== taskId);
      delete draft.executionPlan.positions[taskId];
    });
    setSelectedTaskId(undefined);
    notify(`已删除 ${taskId}`);
  }

  return <div className={`solution-shell ui-scale-${uiScale} ${activeRequirement ? 'with-requirement-sheet' : ''}`}>
    <header className="topbar">
      <div className="traffic-lights" aria-hidden="true"><i/><i/><i/></div>
      <div className="brand-button"><LogoMark/><span><strong>Solution Studio</strong><small>{context.projectName ?? '项目规划'}</small></span></div>
      <div className="document-title"><input value={workspace.title} onChange={(event) => updateWorkspace('workspace', (draft) => { draft.title = event.target.value; })}/><small>{dirtyParts.size > 0 ? '有未保存修改' : `已保存 · v${persistedRevision}`}</small></div>
      <div className="topbar-actions">
        <div className="display-scale" role="group" aria-label="调整界面大小">
          <span>显示</span>
          {([['compact', '小'], ['comfortable', '中'], ['large', '大']] as const).map(([value, label]) => <button key={value} className={uiScale === value ? 'active' : ''} aria-pressed={uiScale === value} onClick={() => setUiScale(value)}>{label}</button>)}
        </div>
        <span className={`service-badge ${repository.mode}`}><i/>{repository.mode === 'server' ? '本地服务' : '浏览器存储'}</span>
        <button className="button secondary" disabled={loadingMarkdown} onClick={() => void openMarkdownPreview()}>{loadingMarkdown ? '正在生成…' : 'Markdown'}</button>
        <button className="button primary" disabled={dirtyParts.size === 0 || saving} onClick={() => void save()}>{saving ? '保存中…' : '保存'}</button>
      </div>
    </header>

    <nav className="workspace-nav" aria-label="规划视图"><div>{navigation.map((item) => <button key={item.id} className={area === item.id ? 'active' : ''} onClick={() => setArea(item.id)}><b>{item.icon}</b><span><strong>{item.label}</strong><small>{item.caption}</small></span></button>)}{context.projectId && <div className="workspace-binding" title={`ChatOS 项目 ID：${context.projectId}${context.projectRoot ? `\n项目目录：${context.projectRoot}` : ''}`}><i/><span>已关联 {context.projectName ?? context.projectId}</span></div>}</div></nav>

    <main className={`content area-${area}`}>
      {area === 'overview' && <Overview workspace={workspace} validation={validation} requirementTrace={requirementsWithDesign} taskTrace={requirementsWithTasks} doneCount={doneCount} readyCount={readyCount} onNavigate={setArea}/>}
      {area === 'requirements' && <RequirementsView workspace={workspace} onAddModule={() => setCreateModal({ kind: 'module' })} onEditModule={(moduleId) => setCreateModal({ kind: 'module', moduleId })} onDeleteModule={deleteModule} onAdd={(moduleId) => setCreateModal({ kind: 'requirement', moduleId })} onOpen={openRequirement}/>}
      {area === 'plan' && <ModulePlanView workspace={workspace} selectedTaskId={selectedTaskId} onSelectTask={setSelectedTaskId} onOpenRequirement={(id, taskId) => { openRequirement(id, 'plan'); setSelectedTaskId(taskId); }} onPositionsChange={(positions) => updateWorkspace('workspace', (draft) => { draft.executionPlan.positions = { ...draft.executionPlan.positions, ...positions }; })}/>}
    </main>

    {activeRequirement && (
      <RequirementSheet
        workspace={workspace}
        requirement={activeRequirement}
        tab={requirementPanelTab}
        selectedTaskId={selectedTaskId}
        onTab={setRequirementPanelTab}
        onSelectTask={setSelectedTaskId}
        onOpenRequirement={(id, taskId) => { openRequirement(id, 'plan'); setSelectedTaskId(taskId); }}
        onClose={() => { setActiveRequirementId(undefined); setSelectedTaskId(undefined); }}
        onAddDesign={() => setCreateModal({ kind: 'design', requirementId: activeRequirement.id })}
        onAddDesignBlock={(designId) => setCreateModal({ kind: 'designBlock', requirementId: activeRequirement.id, designId })}
        onAddTask={() => setCreateModal({ kind: 'task', requirementId: activeRequirement.id })}
        onUpdateDesign={(designId, updater) => updateWorkspace('design', (draft) => { const target = draft.design.sections.find((item) => item.id === designId); if (target) updater(target); })}
        onUpdateTask={updateTask}
        onDeleteTask={deleteTask}
        onPositionsChange={(positions) => updateWorkspace('workspace', (draft) => { draft.executionPlan.positions = { ...draft.executionPlan.positions, ...positions }; })}
      />
    )}
    {createModal && (
      <CreateModal
        state={createModal}
        workspace={workspace}
        onClose={() => setCreateModal(undefined)}
        onSaveModule={saveModule}
        onCreateRequirement={addRequirement}
        onCreateDesign={addDesignSection}
        onCreateDesignBlock={addDesignBlock}
        onCreateTask={addTask}
      />
    )}
    {markdownPreview && <MarkdownPreview title={markdownPreview.title} content={markdownPreview.content} onClose={() => setMarkdownPreview(undefined)} onNotify={notify} onCopyFile={async () => {
      const result = await repository.copyMarkdownFile(workspace.workspaceId);
      notify(`已复制文件 ${result.fileName}`);
    }}/>}
    {toast && <div className="toast">{toast}</div>}
  </div>;
}

function MarkdownPreview({ title, content, onClose, onNotify, onCopyFile }: { title: string; content: string; onClose: () => void; onNotify: (message: string) => void; onCopyFile: () => Promise<void> }) {
  const [showSource, setShowSource] = useState(false);
  const [copyingFile, setCopyingFile] = useState(false);
  useEffect(() => {
    function closeOnEscape(event: KeyboardEvent) { if (event.key === 'Escape') onClose(); }
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [onClose]);
  async function copyContent() {
    try { await navigator.clipboard.writeText(content); onNotify('已复制 Markdown 内容'); }
    catch { onNotify('复制失败，请检查剪贴板权限。'); }
  }
  async function copyFile() {
    if (copyingFile) return;
    setCopyingFile(true);
    try { await onCopyFile(); }
    catch (error) { onNotify(error instanceof Error ? error.message : String(error)); }
    finally { setCopyingFile(false); }
  }
  return <div className="markdown-preview-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <section className="markdown-preview" role="dialog" aria-modal="true" aria-label={`${title} Markdown 预览`}>
      <header><div><span>MARKDOWN</span><h2>{title}</h2><p>在工作台内预览、复制文件或复制原始内容。</p></div><button aria-label="关闭 Markdown 预览" onClick={onClose}>×</button></header>
      <nav><button className={!showSource ? 'active' : ''} onClick={() => setShowSource(false)}>排版预览</button><button className={showSource ? 'active' : ''} onClick={() => setShowSource(true)}>Markdown 源码</button></nav>
      <main>{showSource ? <pre className="markdown-source">{content}</pre> : <article className="markdown-document"><ReactMarkdown components={{ code({ node: _node, className, children, ...props }) { const value = String(children).replace(/\n$/, ''); return className === 'language-svg' && value.trim().startsWith('<svg') ? <img className="markdown-svg" src={svgPreviewUrl(value)} alt="SVG 设计图"/> : <code className={className} {...props}>{children}</code>; } }}>{content}</ReactMarkdown></article>}</main>
      <footer><span>“复制文件”可直接粘贴到 Finder、聊天或上传区域。</span><div><button onClick={() => void copyContent()}>复制内容</button><button className="primary" disabled={copyingFile} onClick={() => void copyFile()}>{copyingFile ? '正在复制…' : '复制文件'}</button></div></footer>
    </section>
  </div>;
}

function Landing({ context, title, creating, onTitle, onCreate }: {
  context: RuntimeContext; title: string; creating: boolean;
  onTitle: (value: string) => void; onCreate: () => void;
}) {
  return <div className="landing-shell">
    <header><div className="traffic-lights" aria-hidden="true"><i/><i/><i/></div><div className="landing-brand"><LogoMark/><strong>Solution Studio</strong></div><span className="service-badge server"><i/>Apple-inspired workspace</span></header>
    <main>
      <section className="hero"><span className="eyebrow">FROM INTENT TO EXECUTION</span><h1>把模糊想法，变成<br/><em>可以执行的方案。</em></h1><p>结合现有项目或全新需求，生成可追踪的需求文档、设计方案和前置依赖计划。</p>
        <div className="create-card"><div className="create-row"><input value={title} onChange={(event) => onTitle(event.target.value)} placeholder="方案名称"/><button disabled={!title.trim() || creating} onClick={onCreate}>{creating ? '正在创建…' : '开始规划'} <span>→</span></button></div>{context.projectName && <small>当前关联项目：{context.projectName}</small>}</div>
      </section>
    </main>
  </div>;
}

function Overview({ workspace, validation, requirementTrace, taskTrace, doneCount, readyCount, onNavigate }: { workspace: SolutionWorkspace; validation?: WorkspaceValidation; requirementTrace: number; taskTrace: number; doneCount: number; readyCount: number; onNavigate: (area: Area) => void }) {
  const totalRequirements = workspace.requirements.items.length;
  const progress = workspace.executionPlan.tasks.length ? Math.round(doneCount / workspace.executionPlan.tasks.length * 100) : 0;
  const projectDesignBlocks = workspace.design.blocks ?? [];
  const [activeProjectBlockId, setActiveProjectBlockId] = useState(projectDesignBlocks[0]?.id);
  const projectBlockIds = projectDesignBlocks.map((block) => block.id).join('|');
  useEffect(() => setActiveProjectBlockId((current) => projectDesignBlocks.some((block) => block.id === current) ? current : projectDesignBlocks[0]?.id), [workspace.workspaceId, projectBlockIds]);
  const activeProjectBlock = projectDesignBlocks.find((block) => block.id === activeProjectBlockId) ?? projectDesignBlocks[0];
  return <div className="document-page overview-page"><div className="page-heading"><span className="eyebrow">PROJECT OVERVIEW</span><h1>{workspace.title}</h1><p>{workspace.description || '需求、设计决策与执行任务集中在同一个可追踪工作区。'}</p></div>
    <div className="metric-grid"><article><span>需求</span><strong>{totalRequirements}</strong><small>{requirementTrace}/{totalRequirements} 已进入设计</small></article><article><span>设计章节</span><strong>{workspace.design.sections.length}</strong><small>{workspace.design.decisions.length} 个关键决策</small></article><article><span>任务进度</span><strong>{progress}%</strong><small>{doneCount}/{workspace.executionPlan.tasks.length} 已完成</small></article><article className="accent"><span>现在可执行</span><strong>{readyCount}</strong><small>全部前置任务已完成</small></article></div>
    <section className="project-brief"><header><div><span className="eyebrow">PROJECT REQUIREMENTS</span><h2>项目背景与总需求</h2></div><button onClick={() => onNavigate('requirements')}>查看 {totalRequirements} 条详细需求 ›</button></header><div className="project-identity"><div><span>项目类型</span><strong>{workspace.projectProfile?.projectType || '未填写'}</strong></div><div><span>交付形态</span><strong>{workspace.projectProfile?.deliveryForm || '未填写'}</strong></div><div><span>目标平台</span><strong>{workspace.projectProfile?.targetPlatforms.join(' · ') || '未填写'}</strong></div></div><div className="project-narrative"><div><strong>项目背景</strong><p>{workspace.projectProfile?.background || '尚未填写项目背景。'}</p></div><div><strong>整体描述</strong><p>{workspace.projectProfile?.overview || workspace.requirements.summary || '尚未填写项目整体描述。'}</p></div></div><div className="project-brief-columns"><div><strong>项目目标</strong>{workspace.requirements.goals.map((item) => <span key={item}>{item}</span>)}</div><div><strong>范围</strong>{workspace.requirements.inScope.map((item) => <span key={item}>{item}</span>)}</div><div><strong>关键约束</strong>{workspace.requirements.constraints.map((item) => <span key={item}>{item}</span>)}</div></div></section>
    <section className="project-design"><header><div><span className="eyebrow">PROJECT DESIGN</span><h2>项目总体设计</h2><p>{workspace.design.summary || '尚未生成项目总体技术方案。'}</p></div></header>{projectDesignBlocks.length ? <><nav className="content-picker">{projectDesignBlocks.map((block) => <button className={block.id === activeProjectBlock?.id ? 'active' : ''} key={block.id} onClick={() => setActiveProjectBlockId(block.id)}><span>{designTypeLabels[block.type]}</span><strong>{block.title}</strong></button>)}</nav><div className="design-content-list">{activeProjectBlock && <DesignBlockPreview block={activeProjectBlock}/>}</div></> : <div className="project-design-empty">缺少项目技术基线和总体架构图。</div>}</section>
    <section className="trace-card"><header><div><strong>这是什么</strong><p>以需求为中心，为每条需求形成一个确定的设计方案，再把方案拆成带前置关系的执行任务。</p></div><span className={validation?.ready ? 'ready' : ''}>{validation?.ready ? '可以交付' : '仍需完善'}</span></header><div className="trace-flow"><button onClick={() => onNavigate('requirements')}><b>{totalRequirements}</b><span>先看需求</span></button><i>→</i><button onClick={() => onNavigate('requirements')}><b>{requirementTrace}</b><span>明确方案</span></button><i>→</i><button onClick={() => onNavigate('requirements')}><b>{taskTrace}</b><span>安排执行</span></button></div></section>
    <section className="issues-card"><header><strong>质量检查</strong><span>{validation?.issues.length ?? 0} 项</span></header>{validation?.issues.length ? validation.issues.slice(0, 6).map((issue) => <div key={`${issue.code}:${issue.path}`}><b>{issue.blocking ? '!' : 'i'}</b><span><strong>{issue.message}</strong><small>{issue.path}</small></span></div>) : <div className="all-good"><b>✓</b><span><strong>结构检查通过</strong><small>没有发现依赖或追踪问题。</small></span></div>}</section>
  </div>;
}

function LineEditor({ label, value, placeholder, onChange }: { label: string; value: string[]; placeholder: string; onChange: (value: string[]) => void }) {
  return <label className="line-editor"><span>{label}</span><textarea rows={Math.max(3, Math.min(7, value.length + 1))} value={value.join('\n')} onChange={(event) => onChange(lines(event.target.value))} placeholder={placeholder}/></label>;
}

function RequirementsView({ workspace, onAddModule, onEditModule, onDeleteModule, onAdd, onOpen }: {
  workspace: SolutionWorkspace;
  onAddModule: () => void;
  onEditModule: (moduleId: string) => void;
  onDeleteModule: (moduleId: string) => void;
  onAdd: (moduleId?: string) => void;
  onOpen: (requirementId: string, tab: RequirementPanelTab) => void;
}) {
  const document = workspace.requirements;
  const children = new Map<string | undefined, RequirementItem[]>();
  for (const item of document.items) children.set(item.parentRequirementId, [...(children.get(item.parentRequirementId) ?? []), item]);
  const parentIds = document.items.filter((item) => (children.get(item.id)?.length ?? 0) > 0).map((item) => item.id);
  const [collapsedIds, setCollapsedIds] = useState<Set<string>>(() => new Set());
  useEffect(() => setCollapsedIds(new Set()), [workspace.workspaceId]);
  const toggleNode = (requirementId: string) => setCollapsedIds((current) => {
    const next = new Set(current);
    if (next.has(requirementId)) next.delete(requirementId);
    else next.add(requirementId);
    return next;
  });
  const itemIds = new Set(document.items.map((item) => item.id));
  const roots = document.items.filter((item) => !item.parentRequirementId || !itemIds.has(item.parentRequirementId));
  const structurallyAttachedIds = new Set<string>();
  function markAttached(item: RequirementItem) {
    if (structurallyAttachedIds.has(item.id)) return;
    structurallyAttachedIds.add(item.id);
    for (const child of children.get(item.id) ?? []) markAttached(child);
  }
  for (const root of roots) markAttached(root);
  const renderRoots = [...roots, ...document.items.filter((item) => !structurallyAttachedIds.has(item.id))];
  const renderedIds = new Set<string>();
  function renderNode(item: RequirementItem, depth: number): React.ReactNode {
    if (renderedIds.has(item.id)) return null;
    renderedIds.add(item.id);
    const childItems = children.get(item.id) ?? [];
    const hasChildren = childItems.length > 0;
    const collapsed = collapsedIds.has(item.id);
    const designs = workspace.design.sections.filter((section) => section.requirementIds.includes(item.id));
    const tasks = workspace.executionPlan.tasks.filter((task) => task.requirementIds.includes(item.id));
    const design = designs[0];
    return <div className={`requirement-tree-node${hasChildren ? ' has-children' : ''}`} role="treeitem" aria-level={depth + 1} aria-expanded={hasChildren ? !collapsed : undefined} key={item.id}>
      <article className="requirement-row">
        <div className="requirement-tree-meta">
          {hasChildren
            ? <button className={`tree-toggle${collapsed ? ' is-collapsed' : ''}`} type="button" aria-label={`${collapsed ? '展开' : '收起'} ${item.id} 的 ${childItems.length} 条子需求`} aria-controls={`requirement-children-${item.id}`} onClick={() => toggleNode(item.id)}><span>›</span></button>
            : <span className="tree-toggle-placeholder"/>}
        </div>
        <button className="requirement-summary" onClick={() => onOpen(item.id, 'detail')}><div><span className="requirement-id">{item.id}</span><h2>{item.title}</h2><span className={`priority priority-${item.priority}`}>{requirementPriorityLabels[item.priority]}</span>{hasChildren && <span className="child-count">{childItems.length} 个子需求</span>}{item.parentRequirementId && <span className="parent-requirement">子需求</span>}</div><p>{item.description}</p><small>{item.acceptanceCriteria.length} 条验收条件{design ? ` · 设计方案「${design.title}」` : ' · 尚未创建设计方案'} · 点击查看详情</small></button>
        <footer><button onClick={() => onOpen(item.id, 'design')}><span>设计方案</span><b>{designs.length}</b><i>›</i></button><button onClick={() => onOpen(item.id, 'plan')}><span>执行计划</span><b>{tasks.length}</b><i>›</i></button></footer>
      </article>
      {hasChildren && !collapsed && <div className="requirement-children" id={`requirement-children-${item.id}`} role="group">{childItems.map((child) => renderNode(child, depth + 1))}</div>}
    </div>;
  }
  const moduleKindLabels = { foundation: '平台基础', business: '业务功能', delivery: '交付保障' } as const;
  const moduleTrees = [...document.modules].sort((left, right) => left.order - right.order).map((module) => ({
    module,
    nodes: renderRoots.filter((item) => item.moduleId === module.id).map((item) => renderNode(item, 0)),
    requirementCount: document.items.filter((item) => item.moduleId === module.id).length,
    taskCount: workspace.executionPlan.tasks.filter((task) => task.moduleId === module.id).length
  }));
  return <div className="document-page requirements-index">
    <div className="requirements-heading">
      <div><span className="eyebrow">MODULES & REQUIREMENTS</span><h1>功能模块</h1><p>先按业务能力组织模块，再在模块内维护需求、设计和执行任务。</p></div>
      <div className="requirements-heading-actions"><button className="button secondary" onClick={onAddModule}>＋ 新增模块</button><button className="button primary" disabled={document.modules.length === 0} onClick={() => onAdd()}>＋ 新增需求</button></div>
    </div>
    <div className="relationship-guide" aria-label="使用方式"><span><b>1</b> 创建需求</span><i>›</i><span><b>2</b> 完成设计方案</span><i>›</i><span><b>3</b> 拆解执行任务</span></div>
    <section className="requirements-list">
      <header><strong>全部模块</strong><div className="requirements-list-tools"><span>{document.modules.length} 个模块 · {document.items.length} 项需求</span>{parentIds.length > 0 && <><button type="button" onClick={() => setCollapsedIds(new Set())}>全部展开</button><button type="button" onClick={() => setCollapsedIds(new Set(parentIds))}>全部收起</button></>}</div></header>
      {document.modules.length > 0 && <div className="module-lanes">{moduleTrees.map(({ module, nodes, requirementCount, taskCount }) => <section className={`module-lane module-${module.kind}`} key={module.id}><header><div><span>{module.id} · {moduleKindLabels[module.kind]}</span><strong>{module.title}</strong><p>{module.description || '尚未补充模块说明'}</p></div><div className="module-lane-actions"><small>{requirementCount} 项需求 · {taskCount} 个任务</small><button onClick={() => onAdd(module.id)}>新增需求</button><button onClick={() => onEditModule(module.id)}>编辑</button><button className="danger" disabled={requirementCount > 0 || taskCount > 0} onClick={() => onDeleteModule(module.id)}>删除</button></div></header>{requirementCount > 0 ? <div className="requirement-tree" role="tree" aria-label={`${module.title}需求层级`}>{nodes}</div> : <div className="requirements-empty compact"><strong>该模块还没有需求</strong><button className="button secondary" onClick={() => onAdd(module.id)}>添加需求</button></div>}</section>)}</div>}
      {document.modules.length === 0 && <div className="requirements-empty"><span>▦</span><strong>先建立功能模块</strong><p>模块是总体设计、需求和任务的稳定边界。创建业务功能模块后，再在模块内拆分需求。</p><button className="button primary" onClick={onAddModule}>新增第一个模块</button></div>}
      {document.modules.length > 0 && document.items.length === 0 && <div className="requirements-empty"><span>◎</span><strong>还没有需求</strong><p>先创建一条清晰的需求。每条需求对应一个确定的设计方案和一份执行计划。</p><button className="button primary" onClick={() => onAdd()}>新增第一条需求</button></div>}
    </section>
  </div>;
}

function ModulePlanView({ workspace, selectedTaskId, onSelectTask, onOpenRequirement, onPositionsChange }: {
  workspace: SolutionWorkspace;
  selectedTaskId?: string;
  onSelectTask: (taskId?: string) => void;
  onOpenRequirement: (requirementId: string, taskId: string) => void;
  onPositionsChange: (positions: Record<string, PlanNodePosition>) => void;
}) {
  const orderedModules = [...workspace.requirements.modules].sort((left, right) => left.order - right.order);
  const [moduleId, setModuleId] = useState(orderedModules[0]?.id ?? '');
  const moduleSignature = orderedModules.map((module) => module.id).join('|');
  useEffect(() => setModuleId((current) => orderedModules.some((module) => module.id === current) ? current : orderedModules[0]?.id ?? ''), [workspace.workspaceId, moduleSignature]);
  const module = orderedModules.find((candidate) => candidate.id === moduleId);
  const primaryTasks = workspace.executionPlan.tasks.filter((task) => task.moduleId === moduleId);
  const primaryTaskIds = new Set(primaryTasks.map((task) => task.id));
  const taskById = new Map(workspace.executionPlan.tasks.map((task) => [task.id, task]));
  const visibleTaskIds = new Set(primaryTaskIds);
  const visitPrerequisites = (task: ExecutionTask) => {
    for (const dependencyId of task.dependsOn) {
      if (visibleTaskIds.has(dependencyId)) continue;
      const dependency = taskById.get(dependencyId);
      if (!dependency) continue;
      visibleTaskIds.add(dependencyId);
      visitPrerequisites(dependency);
    }
  };
  for (const task of primaryTasks) visitPrerequisites(task);
  const plan = { ...workspace.executionPlan, tasks: workspace.executionPlan.tasks.filter((task) => visibleTaskIds.has(task.id)) };
  const selectedTask = workspace.executionPlan.tasks.find((task) => task.id === selectedTaskId && visibleTaskIds.has(task.id));
  const ready = new Set(readyTaskIds(workspace.executionPlan.tasks));
  const readyCount = primaryTasks.filter((task) => ready.has(task.id)).length;
  const doneCount = primaryTasks.filter((task) => task.status === 'done').length;
  return <div className="document-page module-plan-page">
    <div className="page-heading"><span className="eyebrow">MODULE EXECUTION DAG</span><h1>模块任务图</h1><p>按功能模块审查任务主线；跨模块前置会自动带入，但仍保留原模块归属。</p></div>
    {orderedModules.length === 0 ? <div className="requirements-empty"><span>▦</span><strong>还没有功能模块</strong><p>请先在“功能模块”中建立模块和需求。</p></div> : <>
      <nav className="module-plan-tabs" aria-label="选择功能模块">{orderedModules.map((candidate) => {
        const count = workspace.executionPlan.tasks.filter((task) => task.moduleId === candidate.id).length;
        return <button key={candidate.id} className={candidate.id === moduleId ? 'active' : ''} onClick={() => { setModuleId(candidate.id); onSelectTask(undefined); }}><span>{candidate.id}</span><strong>{candidate.title}</strong><small>{count} 个任务</small></button>;
      })}</nav>
      <section className="module-plan-summary"><div><span>当前模块</span><strong>{module?.title}</strong><small>{module?.description || '尚未补充模块说明'}</small></div><div><span>模块任务</span><strong>{primaryTasks.length}</strong><small>{doneCount} 已完成 · {readyCount} 可领取</small></div><div><span>跨模块前置</span><strong>{plan.tasks.length - primaryTasks.length}</strong><small>只读展示完整依赖链</small></div></section>
      <div className="module-plan-workspace"><PlanGraph plan={plan} sourcePlan={workspace.executionPlan} primaryTaskIds={primaryTaskIds} selectedTaskId={selectedTaskId} onSelect={onSelectTask} onPositionsChange={onPositionsChange}/>{selectedTask && <ModuleTaskDetails task={selectedTask} workspace={workspace} primary={primaryTaskIds.has(selectedTask.id)} onClose={() => onSelectTask(undefined)} onOpenRequirement={onOpenRequirement}/>}</div>
    </>}
  </div>;
}

function ModuleTaskDetails({ task, workspace, primary, onClose, onOpenRequirement }: { task: ExecutionTask; workspace: SolutionWorkspace; primary: boolean; onClose: () => void; onOpenRequirement: (requirementId: string, taskId: string) => void }) {
  const owners = workspace.requirements.items.filter((item) => task.requirementIds.includes(item.id));
  const module = workspace.requirements.modules.find((item) => item.id === task.moduleId);
  return <aside className="inspector module-task-inspector"><header><div><span>{task.id}</span><strong>{primary ? '模块任务' : '跨模块前置'}</strong></div><button aria-label="关闭任务详情" onClick={onClose}>×</button></header><div className="inspector-scroll"><h3>{task.title}</h3><p>{task.description || '尚未补充任务说明'}</p><section className="readonly-facts"><p><b>模块</b>{module?.title ?? task.moduleId}</p><p><b>状态</b>{taskStatusLabels[task.status]}</p><p><b>阶段</b>{task.phase || '未分阶段'}</p></section>{task.blockedReason && <section><strong>阻塞原因</strong><p>{task.blockedReason}</p></section>}<section><strong>关联需求</strong>{owners.map((owner) => <button key={owner.id} onClick={() => onOpenRequirement(owner.id, task.id)}>{owner.id} · {owner.title} ›</button>)}{owners.length === 0 && <p>尚未关联需求</p>}</section><section><strong>前置任务</strong>{task.dependsOn.map((id) => { const dependency = workspace.executionPlan.tasks.find((candidate) => candidate.id === id); return <p key={id}>{id} · {dependency?.title ?? '前置不存在'}{dependency ? ` · ${taskStatusLabels[dependency.status]}` : ''}</p>; })}{task.dependsOn.length === 0 && <p>无</p>}</section><section><strong>验收标准</strong>{task.acceptanceCriteria.map((item, index) => <p key={index}>{item}</p>)}</section></div><footer><span>任务状态由“任务执行器”回写</span></footer></aside>;
}

function svgPreviewUrl(svg: string) {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

function DesignBlockPreview({ block }: { block: DesignContentBlock }) {
  return <section className={`design-content-block type-${block.type}`}>
    <header><span>{designTypeLabels[block.type]}</span><strong>{block.title}</strong></header>
    {block.type === 'text' ? <div className="design-text-content">{block.content}</div> : <><img className="svg-preview" src={svgPreviewUrl(block.content)} alt={`${block.title} SVG 预览`}/><details><summary>查看 SVG 源码</summary><pre>{block.content}</pre></details></>}
  </section>;
}

export function RequirementSheet({ workspace, requirement, tab, selectedTaskId, onTab, onSelectTask, onOpenRequirement, onClose, onAddDesign, onAddDesignBlock, onAddTask, onUpdateDesign, onUpdateTask, onDeleteTask, onPositionsChange }: {
  workspace: SolutionWorkspace; requirement: RequirementItem; tab: RequirementPanelTab; selectedTaskId?: string;
  onTab: (tab: RequirementPanelTab) => void; onSelectTask: (id?: string) => void; onClose: () => void; onAddDesign: () => void; onAddDesignBlock: (designId: string) => void; onAddTask: () => void;
  onOpenRequirement: (requirementId: string, taskId: string) => void;
  onUpdateDesign: (id: string, updater: (section: DesignSection) => void) => void; onUpdateTask: (id: string, updater: (task: ExecutionTask) => void) => void; onDeleteTask: (id: string) => void; onPositionsChange: (positions: Record<string, PlanNodePosition>) => void;
}) {
  const designs = workspace.design.sections.filter((section) => section.requirementIds.includes(requirement.id));
  const design = designs.find((section) => section.id === requirement.selectedDesignSectionId) ?? designs[0];
  const [activeBlockId, setActiveBlockId] = useState<string>();
  const [planScope, setPlanScope] = useState<'requirement' | 'complete'>('requirement');
  useEffect(() => {
    setActiveBlockId(undefined);
    setPlanScope('requirement');
  }, [requirement.id, design?.id]);
  const blockIds = design?.blocks?.map((block) => block.id).join('|') ?? '';
  const preferredBlock = design?.blocks?.find((block) => block.type !== 'text') ?? design?.blocks?.[0];
  useEffect(() => setActiveBlockId((current) => design?.blocks?.some((block) => block.id === current) ? current : preferredBlock?.id), [design?.id, blockIds, preferredBlock?.id]);
  const activeBlock = design?.blocks?.find((block) => block.id === activeBlockId) ?? preferredBlock;
  const { tasks, supportingTasks, primaryTaskIds, missingDependencyIds, plan: scopedPlan, readyCount } = requirementPlanView(workspace.executionPlan, requirement.id, planScope);
  const externalDependencyCount = supportingTasks.length;
  const selectedTask = workspace.executionPlan.tasks.find((task) => task.id === selectedTaskId);
  return <div className="requirement-sheet-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <aside className="requirement-sheet" role="dialog" aria-modal="true" aria-label={`${requirement.title} 详情`}>
      <header className="requirement-sheet-header"><div><span>{requirement.id}</span><h2>{requirement.title}</h2><p>{requirement.description}</p></div><button aria-label="关闭" onClick={onClose}>×</button></header>
      <nav className="requirement-sheet-tabs"><button className={tab === 'detail' ? 'active' : ''} onClick={() => onTab('detail')}>需求说明</button><button className={tab === 'design' ? 'active' : ''} onClick={() => onTab('design')}>设计方案 <b>{design ? 1 : 0}</b></button><button className={tab === 'plan' ? 'active' : ''} onClick={() => onTab('plan')}>执行计划 <b>{tasks.length}</b></button></nav>
      <div className="requirement-sheet-body">
        {tab === 'detail' ? <section className="requirement-detail">
          <div className="sheet-section-heading"><div><h3>要解决什么问题</h3><p>设计方案和执行计划都必须回应下面这些可验证结果。</p></div><span className={`priority priority-${requirement.priority}`}>{requirementPriorityLabels[requirement.priority]}</span></div>
          <div className="requirement-detail-description"><span>需求说明</span><p>{requirement.description}</p></div>
          <div className="acceptance-list"><header><strong>验收条件</strong><span>{requirement.acceptanceCriteria.length} 条</span></header>{requirement.acceptanceCriteria.map((criterion, index) => <div key={criterion}><b>{index + 1}</b><span>{criterion}</span></div>)}</div>
          <div className="requirement-detail-next"><div><span>设计方案</span><strong>{design?.title ?? '尚未创建'}</strong></div><button onClick={() => onTab('design')}>{design ? '查看设计方案' : '去创建设计方案'} ›</button></div>
        </section> : tab === 'design' ? <section className="proposal-section">
          <div className="sheet-section-heading"><div><h3>这个需求怎么做</h3><p>每条需求只对应一个确定的设计方案，执行计划直接从该方案拆解。</p></div>{!design && <button className="button primary" onClick={onAddDesign}>＋ 创建设计方案</button>}</div>
          {design ? <>
            <div className="proposal-list"><article>
              <header><span>{design.id}</span></header>
              <input value={design.title} aria-label={`${design.id} 方案名称`} onChange={(event) => onUpdateDesign(design.id, (draft) => { draft.title = event.target.value; })}/>
              <textarea rows={3} value={design.body} aria-label={`${design.id} 方案摘要`} onChange={(event) => onUpdateDesign(design.id, (draft) => { draft.body = event.target.value; })}/>
              <div className="proposal-content-heading"><div><strong>技术设计文档</strong><span>{design.blocks?.length ?? 0} 项内容</span></div><button onClick={() => onAddDesignBlock(design.id)}>＋ 添加内容</button></div>
              {!!design.blocks?.length && <nav className="content-picker">{design.blocks.map((block) => <button className={block.id === activeBlock?.id ? 'active' : ''} key={block.id} onClick={() => setActiveBlockId(block.id)}><span>{designTypeLabels[block.type]}</span><strong>{block.title}</strong></button>)}</nav>}
              <div className="design-content-list">{activeBlock ? <DesignBlockPreview block={activeBlock}/> : <p className="no-design-content">添加文本、架构图、流程图或页面设计图。</p>}</div>
            </article></div>
          </> : <div className="sheet-empty"><span>◇</span><strong>还没有设计方案</strong><p>为这条需求建立唯一的技术设计文档，再据此拆解执行任务。</p><button onClick={onAddDesign}>创建设计方案</button></div>}
        </section> : <section className="scoped-plan">
          <div className="sheet-section-heading"><div><h3>这个需求怎么执行</h3><p>{design ? `当前基于「${design.title}」拆解任务与前置关系${externalDependencyCount ? `，另有 ${externalDependencyCount} 个跨需求前置` : ''}。` : '执行计划需要先完成这条需求的设计方案。'}</p></div><button className="button primary" disabled={!design} onClick={onAddTask}>＋ 新增任务</button></div>
          {!design ? <div className="plan-prerequisite"><span>1</span><div><strong>请先完成设计方案</strong><p>切换到“设计方案”，完成技术设计后再建立执行计划。</p></div><button onClick={() => onTab('design')}>去创建设计方案</button></div> : <>
            {externalDependencyCount > 0 && <div className="plan-scope-toggle"><span>图中范围</span><div><button className={planScope === 'requirement' ? 'active' : ''} onClick={() => setPlanScope('requirement')}>本需求主线 · {tasks.length}</button><button className={planScope === 'complete' ? 'active' : ''} onClick={() => setPlanScope('complete')}>完整依赖 · {tasks.length + supportingTasks.length}</button></div></div>}
            {supportingTasks.length > 0 && <details className="supporting-task-links"><summary>跨需求前置 · {supportingTasks.length}（按需展开）</summary><div aria-label="跨需求前置任务">{supportingTasks.map((task) => <button key={task.id} onClick={() => onSelectTask(task.id)}>{task.id} · {task.title}</button>)}</div></details>}
            {missingDependencyIds.length > 0 && <p className="plan-dependency-warning" role="alert">前置任务不存在：{missingDependencyIds.join('、')}。请修复计划引用；这些任务不会被视为已完成。</p>}
            <div className="sheet-graph"><PlanGraph plan={scopedPlan} sourcePlan={workspace.executionPlan} primaryTaskIds={primaryTaskIds} selectedTaskId={selectedTaskId} onSelect={onSelectTask} onPositionsChange={onPositionsChange}/></div>
            <TaskList tasks={tasks} readyCount={readyCount} onSelect={onSelectTask}/>
            {selectedTask && <div className="sheet-task-editor">{primaryTaskIds.has(selectedTask.id) ? <TaskInspector task={selectedTask} tasks={workspace.executionPlan.tasks} requirements={workspace.requirements.items} sections={workspace.design.sections} onClose={() => onSelectTask(undefined)} onUpdate={(updater) => onUpdateTask(selectedTask.id, updater)} onDelete={() => onDeleteTask(selectedTask.id)}/> : <SupportingTaskDetails task={selectedTask} workspace={workspace} onClose={() => onSelectTask(undefined)} onSelectTask={onSelectTask} onOpenRequirement={onOpenRequirement}/>}</div>}
          </>}
        </section>}
      </div>
    </aside>
  </div>;
}

function SupportingTaskDetails({ task, workspace, onClose, onSelectTask, onOpenRequirement }: {
  task: ExecutionTask; workspace: SolutionWorkspace; onClose: () => void;
  onSelectTask: (id: string) => void; onOpenRequirement: (requirementId: string, taskId: string) => void;
}) {
  const owners = workspace.requirements.items.filter((item) => task.requirementIds.includes(item.id));
  return <aside className="inspector"><header><div><span>{task.id}</span><strong>跨需求支撑任务</strong></div><button aria-label="关闭任务详情" onClick={onClose}>×</button></header><div className="inspector-scroll supporting-task-details">
    <h3>{task.title}</h3><p>{task.description}</p><p>状态：{taskStatusLabels[task.status]}</p>{task.blockedReason && <p>阻塞原因：{task.blockedReason}</p>}
    <section><strong>所属需求</strong>{owners.map((owner) => <button key={owner.id} onClick={() => onOpenRequirement(owner.id, task.id)}>{owner.id} · {owner.title} ›</button>)}{owners.length === 0 && <p>尚未关联需求</p>}</section>
    <section><strong>前置任务</strong>{task.dependsOn.map((id) => { const dependency = workspace.executionPlan.tasks.find((candidate) => candidate.id === id); return dependency ? <button key={id} onClick={() => onSelectTask(id)}>{id} · {dependency.title} · {taskStatusLabels[dependency.status]}</button> : <p key={id}>{id} · 前置不存在</p>; })}{task.dependsOn.length === 0 && <p>无</p>}</section>
    <section><strong>预期输出</strong>{task.deliverables.map((item, index) => <p key={index}>{item}</p>)}</section>
    <section><strong>验收标准</strong>{task.acceptanceCriteria.map((item, index) => <p key={index}>{item}</p>)}</section>
  </div><footer><span>只读查看；进入所属需求后可编辑</span></footer></aside>;
}

function CreateModal({ state, workspace, onClose, onSaveModule, onCreateRequirement, onCreateDesign, onCreateDesignBlock, onCreateTask }: {
  state: CreateModalState; workspace: SolutionWorkspace; onClose: () => void;
  onSaveModule: (input: { moduleId?: string; title: string; description: string; kind: RequirementModuleKind }) => void;
  onCreateRequirement: (input: { moduleId: string; title: string; description: string; priority: 'must' | 'should' | 'could'; parentRequirementId?: string; acceptanceCriteria: string[] }) => void;
  onCreateDesign: (requirementId: string, input: { title: string; body: string }) => void;
  onCreateDesignBlock: (designId: string, input: { type: DesignContentType; title: string; content: string }) => void;
  onCreateTask: (requirementId: string, input: { title: string; description: string; phase: string; acceptanceCriteria: string[]; dependsOn: string[] }) => void;
}) {
  const editedModule = state.kind === 'module' && state.moduleId ? workspace.requirements.modules.find((module) => module.id === state.moduleId) : undefined;
  const [title, setTitle] = useState(editedModule?.title ?? '');
  const [description, setDescription] = useState(editedModule?.description ?? '');
  const [moduleKind, setModuleKind] = useState<RequirementModuleKind>(editedModule?.kind ?? 'business');
  const [moduleId, setModuleId] = useState(state.kind === 'requirement' ? state.moduleId ?? workspace.requirements.modules.find((module) => module.kind === 'business')?.id ?? workspace.requirements.modules[0]?.id ?? '' : '');
  const [priority, setPriority] = useState<'must' | 'should' | 'could'>('must');
  const [parentRequirementId, setParentRequirementId] = useState('');
  const [criteria, setCriteria] = useState('');
  const [phase, setPhase] = useState('实现');
  const [dependsOn, setDependsOn] = useState<string[]>([]);
  const [contentType, setContentType] = useState<DesignContentType>('text');
  const requirement = state.kind === 'design' || state.kind === 'designBlock' || state.kind === 'task' ? workspace.requirements.items.find((item) => item.id === state.requirementId) : undefined;
  const availableTasks = requirement ? workspace.executionPlan.tasks : [];
  const kindLabel = state.kind === 'module' ? '功能模块' : state.kind === 'requirement' ? '需求' : state.kind === 'design' ? '设计方案' : state.kind === 'designBlock' ? '方案内容' : '执行任务';
  const validSvg = state.kind !== 'designBlock' || contentType === 'text' || /<svg[\s>]/i.test(description);
  const valid = Boolean(title.trim() && (state.kind === 'module' || description.trim()) && validSvg && (state.kind !== 'requirement' || (moduleId && lines(criteria).length > 0)));
  function submit() {
    if (!valid) return;
    if (state.kind === 'module') onSaveModule({ ...(state.moduleId ? { moduleId: state.moduleId } : {}), title: title.trim(), description: description.trim(), kind: moduleKind });
    else if (state.kind === 'requirement') onCreateRequirement({ moduleId, title: title.trim(), description: description.trim(), priority, ...(parentRequirementId ? { parentRequirementId } : {}), acceptanceCriteria: lines(criteria) });
    else if (state.kind === 'design') onCreateDesign(state.requirementId, { title: title.trim(), body: description.trim() });
    else if (state.kind === 'designBlock') onCreateDesignBlock(state.designId, { type: contentType, title: title.trim(), content: description.trim() });
    else onCreateTask(state.requirementId, { title: title.trim(), description: description.trim(), phase: phase.trim(), acceptanceCriteria: lines(criteria), dependsOn });
  }
  return <div className="create-modal-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <section className="create-modal" role="dialog" aria-modal="true"><header><div><span>{state.kind === 'module' ? state.moduleId ?? 'NEW MODULE' : state.kind === 'requirement' ? 'NEW REQUIREMENT' : requirement?.id}</span><h2>{state.kind === 'module' && state.moduleId ? '编辑' : '新增'}{kindLabel}</h2><p>{state.kind === 'module' ? '模块定义稳定的业务或技术边界，需求、设计和任务都归属于模块。' : state.kind === 'requirement' ? '写清用户需要的结果，后续方案和计划都会归在这条需求下。' : `归属于：${requirement?.title ?? ''}`}</p></div><button aria-label="关闭" onClick={onClose}>×</button></header>
      <div className="modal-fields">
        {state.kind === 'module' && <label><span>模块类型</span><select value={moduleKind} onChange={(event) => setModuleKind(event.target.value as RequirementModuleKind)}><option value="business">业务功能</option><option value="foundation">平台基础</option><option value="delivery">交付保障</option></select></label>}
        {state.kind === 'designBlock' && <label><span>内容类型</span><select value={contentType} onChange={(event) => setContentType(event.target.value as DesignContentType)}><option value="text">文本</option><option value="architecture">架构图 · SVG</option><option value="flowchart">流程图 · SVG</option><option value="ui-svg">页面设计图 · SVG</option></select></label>}
        <label><span>{kindLabel}名称</span><input autoFocus value={title} onChange={(event) => setTitle(event.target.value)} placeholder={`输入${kindLabel}名称`}/></label>
        <label><span>{state.kind === 'module' ? '模块说明' : state.kind === 'design' ? '方案摘要' : state.kind === 'designBlock' ? contentType === 'text' ? '文本内容' : 'SVG 代码' : '详细说明'}</span><textarea className={state.kind === 'designBlock' && contentType !== 'text' ? 'code-input' : ''} rows={state.kind === 'designBlock' ? 10 : 5} value={description} onChange={(event) => setDescription(event.target.value)} placeholder={state.kind === 'module' ? '说明模块职责、边界和不负责的内容' : state.kind === 'design' ? '说明实现方式、关键取舍和边界' : state.kind === 'designBlock' ? contentType === 'text' ? '输入方案正文' : '<svg viewBox="0 0 1200 800" …>…</svg>' : '说明目标、边界和预期结果'}/>{state.kind === 'designBlock' && contentType !== 'text' && !validSvg && description.trim() && <small className="field-error">请输入完整的 SVG 代码。</small>}</label>
        {state.kind === 'requirement' && <><label><span>所属模块</span><select value={moduleId} onChange={(event) => { setModuleId(event.target.value); setParentRequirementId(''); }}><option value="">请选择模块</option>{[...workspace.requirements.modules].sort((left, right) => left.order - right.order).map((module) => <option key={module.id} value={module.id}>{module.id} · {module.title}</option>)}</select></label><label><span>父需求 · 可选</span><select value={parentRequirementId} onChange={(event) => setParentRequirementId(event.target.value)}><option value="">顶层需求</option>{workspace.requirements.items.filter((item) => item.moduleId === moduleId).map((item) => <option key={item.id} value={item.id}>{item.id} · {item.title}</option>)}</select></label><label><span>优先级</span><select value={priority} onChange={(event) => setPriority(event.target.value as typeof priority)}><option value="must">高</option><option value="should">中</option><option value="could">低</option></select></label><label><span>验收条件 · 每行一条</span><textarea rows={4} value={criteria} onChange={(event) => setCriteria(event.target.value)} placeholder="用户可以完成…&#10;系统能够正确…"/></label></>}
        {state.kind === 'task' && <><label><span>阶段</span><input value={phase} onChange={(event) => setPhase(event.target.value)} placeholder="例如：准备、实现、验证"/></label><label><span>验收条件 · 每行一条</span><textarea rows={3} value={criteria} onChange={(event) => setCriteria(event.target.value)} placeholder="任务完成的可验证标准"/></label><fieldset><legend>前置任务</legend>{availableTasks.length ? availableTasks.map((task) => <label className="modal-check" key={task.id}><input type="checkbox" checked={dependsOn.includes(task.id)} onChange={(event) => setDependsOn((current) => event.target.checked ? [...current, task.id] : current.filter((id) => id !== task.id))}/><span><b>{task.id} · {task.title}</b><small>完成后当前任务才能开始</small></span></label>) : <p>这是第一个任务，不需要前置任务。</p>}</fieldset></>}
      </div>
      <footer className="modal-actions"><button onClick={onClose}>取消</button><button className="button primary" disabled={!valid} onClick={submit}>{state.kind === 'module' && state.moduleId ? '保存模块' : `创建${kindLabel}`}</button></footer>
    </section>
  </div>;
}

function TaskList({ tasks, readyCount, onSelect }: { tasks: ExecutionTask[]; readyCount: number; onSelect: (id: string) => void }) {
  return <div className="task-list"><header><span>任务</span><span>阶段</span><span>前置</span><span>状态</span></header>{tasks.map((task) => <button key={task.id} onClick={() => onSelect(task.id)}><span><b>{task.id}</b><strong>{task.title}</strong></span><span>{task.phase || '—'}</span><span>{task.dependsOn.join(', ') || '无'}</span><em className={`task-status ${task.status}`}>{taskStatusLabels[task.status]}</em></button>)}{tasks.length === 0 && <div className="empty-list">还没有任务。添加第一个执行任务后，可设置前置关系。</div>}<footer>本需求 {readyCount} 个任务具备开始条件（按完整前置计算，不代表已启动执行）</footer></div>;
}

function TaskInspector({ task, tasks, requirements, sections, onClose, onUpdate, onDelete }: { task: ExecutionTask; tasks: ExecutionTask[]; requirements: Array<{ id: string; title: string }>; sections: Array<{ id: string; title: string }>; onClose: () => void; onUpdate: (updater: (task: ExecutionTask) => void) => void; onDelete: () => void }) {
  const editable = task.status === 'planned';
  return <aside className="inspector"><header><div><span>{task.id}</span><strong>{editable ? '任务详情' : '任务详情 · 执行后只读'}</strong></div><button onClick={onClose}>×</button></header><div className="inspector-scroll"><label><span>任务名称</span><input value={task.title} readOnly={!editable} onChange={(event) => onUpdate((draft) => { draft.title = event.target.value; })}/></label><label><span>说明</span><textarea rows={4} value={task.description} readOnly={!editable} onChange={(event) => onUpdate((draft) => { draft.description = event.target.value; })}/></label><div className="two-fields"><label><span>执行状态 · 由任务执行器回写</span><input value={taskStatusLabels[task.status]} readOnly/></label><label><span>阶段</span><input value={task.phase} readOnly={!editable} onChange={(event) => onUpdate((draft) => { draft.phase = event.target.value; })}/></label></div>{task.status === 'blocked' && <label><span>阻塞原因 · 只读</span><textarea rows={3} value={task.blockedReason ?? ''} readOnly/></label>}<section className="inspector-section"><strong>前置任务</strong><p>只有全部前置任务完成后，当前任务才会进入“可以开始”。</p>{tasks.filter((candidate) => candidate.id !== task.id).map((candidate) => <label className="check-row" key={candidate.id}><input type="checkbox" disabled={!editable} checked={task.dependsOn.includes(candidate.id)} onChange={(event) => onUpdate((draft) => { draft.dependsOn = event.target.checked ? [...draft.dependsOn, candidate.id] : draft.dependsOn.filter((id) => id !== candidate.id); })}/><span><b>{candidate.id}</b>{candidate.title}</span></label>)}</section><section className="inspector-section"><strong>关联需求</strong>{requirements.map((item) => <label className="check-row" key={item.id}><input type="checkbox" disabled={!editable} checked={task.requirementIds.includes(item.id)} onChange={(event) => onUpdate((draft) => { draft.requirementIds = event.target.checked ? [...draft.requirementIds, item.id] : draft.requirementIds.filter((id) => id !== item.id); })}/><span><b>{item.id}</b>{item.title}</span></label>)}</section><section className="inspector-section"><strong>关联设计</strong>{sections.map((item) => <label className="check-row" key={item.id}><input type="checkbox" disabled={!editable} checked={task.designSectionIds.includes(item.id)} onChange={(event) => onUpdate((draft) => { draft.designSectionIds = event.target.checked ? [...draft.designSectionIds, item.id] : draft.designSectionIds.filter((id) => id !== item.id); })}/><span><b>{item.id}</b>{item.title}</span></label>)}</section>{editable ? <LineEditor label="验收条件" value={task.acceptanceCriteria} placeholder="每行一个可验证结果" onChange={(value) => onUpdate((draft) => { draft.acceptanceCriteria = value; })}/> : <section className="inspector-section"><strong>验收条件</strong>{task.acceptanceCriteria.map((item, index) => <p key={index}>{item}</p>)}</section>}</div><footer>{editable ? <button className="danger-button" onClick={onDelete}>删除任务</button> : <span className="readonly-note">状态和定义已冻结，由任务执行器继续处理</span>}</footer></aside>;
}

function LogoMark() {
  return <span className="logo-mark" aria-hidden="true"><i/><i/><i/><b>✓</b></span>;
}
