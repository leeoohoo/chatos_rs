import type { ReactNode } from 'react';
import { diagramTypeCatalog } from '../../src/templates';
import type { DiagramNode } from '../../src/schema';
import { Icon } from './Icons';
import { kindIcon } from './DiagramStudioSupport';

export interface DiagramStudioSheets {
  newProjectSheet: ReactNode;
  newDiagramSheet: ReactNode;
  mindmapEditSheet: ReactNode;
  deleteDocumentSheet: ReactNode;
}

export function renderDiagramStudioSheets(context: Record<string, any>): DiagramStudioSheets {
  const {
    newProjectVisible,
    setNewProjectVisible,
    newProjectName,
    setNewProjectName,
    createProject,
    newDiagramVisible,
    setNewDiagramVisible,
    activeProject,
    newDiagramName,
    setNewDiagramName,
    createBlankDiagram,
    mindmapEdit,
    setMindmapEdit,
    document,
    updateNode,
    deleteDocumentTarget,
    isDeletingDocument,
    setDeleteDocumentTarget,
    dirty,
    deleteRequestedDocument
  } = context;

  const newProjectSheet = newProjectVisible && <div className="sheet-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) setNewProjectVisible(false); }}>
    <section className="new-project-sheet" role="dialog" aria-modal="true" aria-labelledby="new-project-title">
      <div className="sheet-heading">
        <div><strong id="new-project-title">新建用户项目</strong><span>项目用于归类和管理多张图形</span></div>
        <button className="icon-button subtle" onClick={() => setNewProjectVisible(false)} aria-label="关闭新建项目"><Icon name="close" /></button>
      </div>
      <div className="project-name-section">
        <label htmlFor="new-project-name">项目名称</label>
        <input id="new-project-name" autoFocus value={newProjectName} onChange={(event) => setNewProjectName(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && newProjectName.trim()) void createProject(); }} placeholder="例如：Chatos 客户端" maxLength={240} />
      </div>
      <div className="project-create-footer"><button className="toolbar-button" onClick={() => setNewProjectVisible(false)}>取消</button><button className="toolbar-button primary" disabled={!newProjectName.trim()} onClick={() => void createProject()}>创建项目</button></div>
    </section>
  </div>;

  const newDiagramSheet = newDiagramVisible && <div className="sheet-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) setNewDiagramVisible(false); }}>
    <section className="new-diagram-sheet" role="dialog" aria-modal="true" aria-labelledby="new-diagram-title">
      <div className="sheet-heading">
        <div><strong id="new-diagram-title">新建图形</strong><span>图形将保存在“{activeProject?.name}”项目中</span></div>
        <button className="icon-button subtle" onClick={() => setNewDiagramVisible(false)} aria-label="关闭新建图形"><Icon name="close" /></button>
      </div>
      <div className="project-name-section">
        <label htmlFor="new-diagram-name">图形名称</label>
        <input id="new-diagram-name" autoFocus value={newDiagramName} onChange={(event) => setNewDiagramName(event.target.value)} placeholder="例如：登录认证时序" maxLength={240} />
      </div>
      <div className="template-section-heading"><strong>选择图形类型</strong><span>创建后进入空白画布</span></div>
      <div className="template-grid">
        {diagramTypeCatalog.map((diagramType) => (
          <button key={diagramType.kind} disabled={!newDiagramName.trim()} onClick={() => void createBlankDiagram(diagramType.kind)}>
            <span className={`new-template-icon kind-${diagramType.kind}`}><Icon name={kindIcon(diagramType.kind)} /></span>
            <span><strong>{diagramType.title}</strong><small>{diagramType.subtitle}</small></span>
            <Icon name="chevron" className="template-chevron" />
          </button>
        ))}
      </div>
      <p className="sheet-footnote">只设置图形类型，不会自动生成任何节点、文字或连线。</p>
    </section>
  </div>;

  const mindmapEditSheet = mindmapEdit && <div className="sheet-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) setMindmapEdit(undefined); }}>
    <section className="mindmap-edit-sheet" role="dialog" aria-modal="true" aria-labelledby="mindmap-edit-title">
      <div className="sheet-heading">
        <div><strong id="mindmap-edit-title">编辑主题</strong><span>主题使用短语表达一个概念，不要放入整段说明</span></div>
        <button className="icon-button subtle" onClick={() => setMindmapEdit(undefined)} aria-label="关闭主题编辑"><Icon name="close" /></button>
      </div>
      <div className="project-name-section">
        <label htmlFor="mindmap-topic-name">主题文字</label>
        <input id="mindmap-topic-name" autoFocus value={mindmapEdit.value} onChange={(event) => setMindmapEdit({ ...mindmapEdit, value: event.target.value })} onKeyDown={(event) => {
          if (event.key === 'Enter' && mindmapEdit.value.trim()) {
            event.preventDefault();
            const node = document?.nodes.find((candidate: DiagramNode) => candidate.id === mindmapEdit.nodeId);
            if (node) updateNode({ ...node, data: { ...node.data, label: mindmapEdit.value.trim().slice(0, 80) } });
            setMindmapEdit(undefined);
          }
        }} placeholder="输入主题" maxLength={80} />
      </div>
      <div className="project-create-footer"><button className="toolbar-button" onClick={() => setMindmapEdit(undefined)}>取消</button><button className="toolbar-button primary" disabled={!mindmapEdit.value.trim()} onClick={() => {
        const node = document?.nodes.find((candidate: DiagramNode) => candidate.id === mindmapEdit.nodeId);
        if (node) updateNode({ ...node, data: { ...node.data, label: mindmapEdit.value.trim().slice(0, 80) } });
        setMindmapEdit(undefined);
      }}>完成</button></div>
    </section>
  </div>;

  const deleteDocumentSheet = deleteDocumentTarget && <div className="sheet-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget && !isDeletingDocument) setDeleteDocumentTarget(undefined); }}>
    <section className="confirm-sheet" role="alertdialog" aria-modal="true" aria-labelledby="delete-document-title">
      <span className="confirm-icon destructive"><Icon name="trash" /></span>
      <div><strong id="delete-document-title">删除图形？</strong><p>“{deleteDocumentTarget.title}”将从当前项目中永久删除，此操作无法撤销。{document?.documentId === deleteDocumentTarget.documentId && dirty ? ' 未保存的修改也会一并丢失。' : ''}</p></div>
      <div className="confirm-actions">
        <button className="toolbar-button" disabled={isDeletingDocument} onClick={() => setDeleteDocumentTarget(undefined)}>取消</button>
        <button className="toolbar-button destructive-primary" disabled={isDeletingDocument} onClick={() => void deleteRequestedDocument()}><Icon name="trash" />{isDeletingDocument ? '正在删除…' : '删除图形'}</button>
      </div>
    </section>
  </div>;

  return { newProjectSheet, newDiagramSheet, mindmapEditSheet, deleteDocumentSheet };
}
